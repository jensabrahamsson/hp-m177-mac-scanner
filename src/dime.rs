//! Direct Internet Message Encapsulation (draft-nielsen-dime-02).
//!
//! HP's SOAP `RetrieveImage` response on this MFP is `Content-Type:
//! application/dime`: a SOAP XML record followed by one or more image
//! records (often chunked).

use crate::error::{Error, Result};

const TYPE_MEDIA: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimeRecord {
    pub first: bool,
    pub last: bool,
    pub chunked: bool,
    pub type_format: u8,
    pub id: Vec<u8>,
    pub typ: Vec<u8>,
    pub data: Vec<u8>,
}

impl DimeRecord {
    pub fn media(typ: &str, data: Vec<u8>) -> Self {
        Self {
            first: false,
            last: false,
            chunked: false,
            type_format: TYPE_MEDIA,
            id: Vec::new(),
            typ: typ.as_bytes().to_vec(),
            data,
        }
    }

    pub fn type_str(&self) -> String {
        String::from_utf8_lossy(&self.typ).into_owned()
    }
}

pub fn pad4(n: usize) -> usize {
    (4 - (n % 4)) % 4
}

/// Encode a complete DIME message. Sets MB on the first record and ME on the last.
pub fn encode(records: &[DimeRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    let n = records.len();
    for (i, rec) in records.iter().enumerate() {
        let first = i == 0 || rec.first;
        let last = i + 1 == n || rec.last;
        out.extend_from_slice(&encode_record(rec, first, last));
    }
    out
}

pub fn encode_record(rec: &DimeRecord, first: bool, last: bool) -> Vec<u8> {
    let mut out = Vec::new();
    // version=1 (5 bits), MB, ME, CF
    let b0 = (1u8 << 3)
        | (u8::from(first) << 2)
        | (u8::from(last) << 1)
        | u8::from(rec.chunked);
    let b1 = rec.type_format << 4;
    out.push(b0);
    out.push(b1);
    out.extend_from_slice(&0u16.to_be_bytes()); // options length
    out.extend_from_slice(&(rec.id.len() as u16).to_be_bytes());
    out.extend_from_slice(&(rec.typ.len() as u16).to_be_bytes());
    out.extend_from_slice(&(rec.data.len() as u32).to_be_bytes());
    out.extend_from_slice(&rec.id);
    out.extend(std::iter::repeat(0u8).take(pad4(rec.id.len())));
    out.extend_from_slice(&rec.typ);
    out.extend(std::iter::repeat(0u8).take(pad4(rec.typ.len())));
    out.extend_from_slice(&rec.data);
    out.extend(std::iter::repeat(0u8).take(pad4(rec.data.len())));
    out
}

/// Decode every DIME record in `bytes`.
pub fn decode(mut bytes: &[u8]) -> Result<Vec<DimeRecord>> {
    let mut recs = Vec::new();
    while !bytes.is_empty() {
        if bytes.len() < 12 {
            return Err(Error::protocol(format!(
                "truncated DIME header ({} leftover bytes)",
                bytes.len()
            )));
        }
        let b0 = bytes[0];
        let version = b0 >> 3;
        if version != 1 {
            return Err(Error::protocol(format!("unsupported DIME version {version}")));
        }
        let first = (b0 & 0x04) != 0;
        let last = (b0 & 0x02) != 0;
        let chunked = (b0 & 0x01) != 0;
        let type_format = bytes[1] >> 4;
        let opt_len = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
        let id_len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        let typ_len = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;
        let data_len = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        let mut off = 12;
        let take = |buf: &[u8], off: &mut usize, n: usize| -> Result<Vec<u8>> {
            if *off + n > buf.len() {
                return Err(Error::protocol("truncated DIME record payload"));
            }
            let slice = buf[*off..*off + n].to_vec();
            *off += n;
            *off += pad4(n);
            if *off > buf.len() {
                return Err(Error::protocol("truncated DIME padding"));
            }
            Ok(slice)
        };
        let _opts = take(bytes, &mut off, opt_len)?;
        let id = take(bytes, &mut off, id_len)?;
        let typ = take(bytes, &mut off, typ_len)?;
        let data = take(bytes, &mut off, data_len)?;
        recs.push(DimeRecord {
            first,
            last,
            chunked,
            type_format,
            id,
            typ,
            data,
        });
        bytes = &bytes[off.min(bytes.len())..];
        if last {
            break;
        }
    }
    if recs.is_empty() {
        return Err(Error::protocol("DIME message contained no records"));
    }
    Ok(recs)
}

fn is_image_type(rec: &DimeRecord) -> bool {
    let t = rec.type_str().to_ascii_lowercase();
    t.contains("jpeg")
        || t.contains("jpg")
        || t.contains("jfif")
        || t.contains("image/")
        || t.contains("octet-stream")
        || t.contains("pdf")
}

/// Concatenate image records. Continuation records (CF) with empty TYPE/ID
/// are appended to the preceding image payload.
pub fn extract_image(bytes: &[u8]) -> Result<Vec<u8>> {
    let recs = decode(bytes)?;
    let mut image = Vec::new();
    let mut collecting = false;
    for rec in &recs {
        if is_image_type(rec) {
            if image.is_empty() {
                image.extend_from_slice(&rec.data);
                collecting = rec.chunked;
            }
            // Extra CF-clear typed image records are separate parts, not glued.
        } else if collecting && rec.typ.is_empty() {
            image.extend_from_slice(&rec.data);
            collecting = rec.chunked;
        }
    }
    if image.is_empty() {
        return Err(Error::protocol(
            "DIME message had no image part (expected image/jpeg after the SOAP record)",
        ));
    }
    Ok(image)
}

/// SOAP record plus a JPEG split into CF continuation records.
pub fn wrap_soap_and_jpeg_chunked(soap_xml: &[u8], jpeg: &[u8], chunk: usize) -> Vec<u8> {
    let chunk = chunk.max(1);
    let mut recs = vec![DimeRecord::media("text/xml", soap_xml.to_vec())];
    let parts: Vec<&[u8]> = jpeg.chunks(chunk).collect();
    for (i, part) in parts.iter().enumerate() {
        let last_img = i + 1 == parts.len();
        recs.push(DimeRecord {
            first: false,
            last: false,
            chunked: !last_img,
            type_format: if i == 0 { TYPE_MEDIA } else { 0 },
            id: Vec::new(),
            typ: if i == 0 {
                b"image/jpeg".to_vec()
            } else {
                Vec::new()
            },
            data: part.to_vec(),
        });
    }
    encode(&recs)
}

/// Build a SOAP+JPEG DIME body the way this firmware does.
pub fn wrap_soap_and_jpeg(soap_xml: &[u8], jpeg: &[u8]) -> Vec<u8> {
    encode(&[
        DimeRecord::media("text/xml", soap_xml.to_vec()),
        DimeRecord::media("image/jpeg", jpeg.to_vec()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_fixture_extracts_jpeg() {
        let raw = include_bytes!("../fixtures/dime-jpeg.bin");
        let recs = decode(raw).expect("decode spec fixture");
        assert_eq!(recs.len(), 2);
        assert!(recs[0].first);
        assert!(!recs[0].last);
        assert!(recs[1].last);
        assert_eq!(recs[0].type_str(), "text/xml");
        assert_eq!(recs[1].type_str(), "image/jpeg");
        let jpeg = extract_image(raw).unwrap();
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
        assert_eq!(&jpeg[jpeg.len() - 2..], &[0xff, 0xd9]);
    }

    #[test]
    fn roundtrip_two_records() {
        let body = wrap_soap_and_jpeg(b"<ok/>", &[0xff, 0xd8, 0x00, 0xff, 0xd9]);
        let recs = decode(&body).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[1].data, [0xff, 0xd8, 0x00, 0xff, 0xd9]);
    }

    #[test]
    fn chunked_continuation_is_concatenated() {
        let jpeg = [0xff, 0xd8, 1, 2, 3, 4, 0xff, 0xd9];
        let body = wrap_soap_and_jpeg_chunked(b"<ok/>", &jpeg, 3);
        let recs = decode(&body).unwrap();
        assert!(recs.iter().any(|r| r.chunked), "CF bit must be set on a chunk");
        let got = extract_image(&body).unwrap();
        assert_eq!(got, jpeg);
    }

    #[test]
    fn chunked_fixture_is_consumed_by_shipped_extractor() {
        let raw = include_bytes!("../fixtures/dime-jpeg-chunked.bin");
        let jpeg = extract_image(raw).unwrap();
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
        assert_eq!(&jpeg[jpeg.len() - 2..], &[0xff, 0xd9]);
        assert!(jpeg.len() > 4);
    }

    #[test]
    fn cf_clear_second_typed_record_is_not_glued() {
        let first = [0xff, 0xd8, 1, 2, 0xff, 0xd9];
        let second = [0xff, 0xd8, 9, 9, 9, 0xff, 0xd9];
        let body = encode(&[
            DimeRecord::media("text/xml", b"<ok/>".to_vec()),
            DimeRecord::media("image/jpeg", first.to_vec()),
            DimeRecord::media("image/jpeg", second.to_vec()),
        ]);
        let got = extract_image(&body).unwrap();
        assert_eq!(got, first, "only the first CF-clear image record");
    }
}
