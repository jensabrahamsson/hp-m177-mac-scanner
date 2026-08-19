//! JPEG validation / generation and a one-page PDF wrapper.

use crate::error::{Error, Result};
use crate::model::ColorMode;

pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes.starts_with(&[0xff, 0xd8]) && find_eoi(bytes).is_some()
}

pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF") && bytes.windows(5).any(|w| w == b"%%EOF")
}

fn find_eoi(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).rposition(|w| w == [0xff, 0xd9])
}

/// Read width/height from SOF0/SOF1/SOF2.
pub fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 4 < bytes.len() {
        if bytes[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if marker == 0xd8 || marker == 0xd9 || (0xd0..=0xd7).contains(&marker) {
            i += 2;
            continue;
        }
        if i + 4 > bytes.len() {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if matches!(marker, 0xc0 | 0xc1 | 0xc2) && i + 9 < bytes.len() {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// Tiny but structurally valid JPEG used by the fake device. A COM marker
/// carries the job parameters so tests can inspect the payload without
/// depending on pixel dumps.
pub fn synthetic_jpeg(comment: &str, color: ColorMode) -> Vec<u8> {
    let mut com = Vec::from(b"\xff\xfe".as_slice());
    let payload = comment.as_bytes();
    let len = (payload.len() + 2) as u16;
    com.extend_from_slice(&len.to_be_bytes());
    com.extend_from_slice(payload);

    // 8×8 SOF0, 1 component (gray) or 3 (color). Entropy is a stub; we only
    // promise SOI/SOF/EOI so validators and the PDF wrapper can work.
    let mut sof = vec![0xff, 0xc0];
    let nf: u8 = if color == ColorMode::Color { 3 } else { 1 };
    let sof_len = 8 + 3 * nf as u16;
    sof.extend_from_slice(&sof_len.to_be_bytes());
    sof.push(8);
    sof.extend_from_slice(&8u16.to_be_bytes());
    sof.extend_from_slice(&8u16.to_be_bytes());
    sof.push(nf);
    for c in 1..=nf {
        sof.push(c);
        sof.push(0x11);
        sof.push(0);
    }
    let sos = [
        0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x00,
    ];
    let mut out = vec![0xff, 0xd8];
    out.extend_from_slice(&com);
    out.extend_from_slice(&sof);
    out.extend_from_slice(&sos);
    out.extend_from_slice(&[0xff, 0xd9]);
    out
}

pub fn jpeg_comment(bytes: &[u8]) -> Option<String> {
    let mut i = 2;
    while i + 4 < bytes.len() {
        if bytes[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if marker == 0xfe {
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            if i + 2 + len <= bytes.len() && len >= 2 {
                return Some(String::from_utf8_lossy(&bytes[i + 4..i + 2 + len]).into_owned());
            }
        }
        if marker == 0xd8 || marker == 0xd9 {
            i += 2;
            continue;
        }
        if i + 4 > bytes.len() {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        i += 2 + len;
    }
    None
}

/// Wrap a JPEG in a one-page PDF using `/Filter /DCTDecode`.
pub fn jpeg_to_pdf(jpeg: &[u8]) -> Result<Vec<u8>> {
    if !is_jpeg(jpeg) {
        return Err(Error::protocol("cannot wrap non-JPEG bytes as PDF"));
    }
    let (w, h) = jpeg_dimensions(jpeg).unwrap_or((8, 8));
    // PDF user space is 72 dpi; we do not need a physical size for validity.
    let pw = w as f64;
    let ph = h as f64;
    let content = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im0 Do Q\n");
    let mut pdf = Vec::new();
    let mut offsets = Vec::new();
    let obj = |pdf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(body);
    };
    pdf.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");
    obj(
        &mut pdf,
        &mut offsets,
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
    );
    obj(
        &mut pdf,
        &mut offsets,
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
    );
    let page = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
         /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>\nendobj\n"
    );
    obj(&mut pdf, &mut offsets, page.as_bytes());
    let colorspace = if jpeg_is_gray(jpeg) {
        "/DeviceGray"
    } else {
        "/DeviceRGB"
    };
    let img_header = format!(
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
         /ColorSpace {colorspace} /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        jpeg.len()
    );
    offsets.push(pdf.len());
    pdf.extend_from_slice(img_header.as_bytes());
    pdf.extend_from_slice(jpeg);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let stream = format!(
        "5 0 obj\n<< /Length {} >>\nstream\n{content}endstream\nendobj\n",
        content.len()
    );
    obj(&mut pdf, &mut offsets, stream.as_bytes());
    let xref_at = pdf.len();
    let count = offsets.len() + 1;
    pdf.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n")
            .as_bytes(),
    );
    Ok(pdf)
}

fn jpeg_is_gray(bytes: &[u8]) -> bool {
    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if matches!(marker, 0xc0 | 0xc1 | 0xc2) {
            return bytes[i + 9] == 1;
        }
        if i + 4 > bytes.len() {
            break;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        i += 2 + len;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_jpeg_and_pdf_are_structurally_valid() {
        let jpg = synthetic_jpeg("source=Platen color=RGB24 dpi=300", ColorMode::Color);
        assert!(is_jpeg(&jpg));
        assert_eq!(jpeg_dimensions(&jpg), Some((8, 8)));
        assert_eq!(
            jpeg_comment(&jpg).as_deref(),
            Some("source=Platen color=RGB24 dpi=300")
        );
        let pdf = jpeg_to_pdf(&jpg).unwrap();
        assert!(is_pdf(&pdf));
        assert!(pdf.windows(b"/Type /Page".len()).any(|w| w == b"/Type /Page"));
    }
}
