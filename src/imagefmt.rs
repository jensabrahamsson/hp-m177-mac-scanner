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
    let nf: u8 = if color == ColorMode::Color { 3 } else { 1 }; // gray + lineart
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

pub fn is_bmp(bytes: &[u8]) -> bool {
    bytes.len() >= 54 && bytes.starts_with(b"BM")
}

pub fn is_dib(bytes: &[u8]) -> bool {
    if bytes.len() < 40 {
        return false;
    }
    let header = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    header == 40 || header == 108 || header == 124
}

pub fn is_tiff(bytes: &[u8]) -> bool {
    bytes.len() >= 4
        && (bytes.starts_with(&[b'I', b'I', 42, 0]) || bytes.starts_with(&[b'M', b'M', 0, 42]))
}

/// 32-bit top-down BGRA BMP used by tests and the WSD stand-in.
pub fn solid_bmp_bgra(width: u32, height: u32, b: u8, g: u8, r: u8) -> Vec<u8> {
    let row = width as usize * 4;
    let pixel_bytes = row * height as usize;
    let file_size = (14 + 40 + pixel_bytes) as u32;
    let mut out = Vec::with_capacity(file_size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(-(height as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    out.extend_from_slice(&11811u32.to_le_bytes());
    out.extend_from_slice(&11811u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..height as usize * width as usize {
        out.extend_from_slice(&[b, g, r, 0]);
    }
    out
}

pub fn raster_to_jpeg(bytes: &[u8]) -> Result<Vec<u8>> {
    let (w, h, rgb) = decode_bmp_or_dib(bytes)?;
    rgb_to_jpeg(&rgb, w, h, 80)
}

pub fn rgb_to_jpeg(rgb: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    if width == 0 || height == 0 || width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err(Error::protocol("JPEG dimensions out of range"));
    }
    let expected = width as usize * height as usize * 3;
    if rgb.len() < expected {
        return Err(Error::protocol("RGB buffer shorter than width×height"));
    }
    let mut out = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    encoder
        .encode(
            &rgb[..expected],
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|e| Error::protocol(format!("jpeg encode: {e}")))?;
    Ok(out)
}

pub fn decode_bmp_or_dib(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    if is_bmp(bytes) {
        let pixel_off = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
        parse_dib(&bytes[14..], bytes, pixel_off)
    } else if is_dib(bytes) {
        let header = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        parse_dib(bytes, bytes, header)
    } else {
        Err(Error::protocol("not a BMP/DIB image"))
    }
}

fn parse_dib(info: &[u8], file: &[u8], pixel_off: usize) -> Result<(u32, u32, Vec<u8>)> {
    if info.len() < 40 {
        return Err(Error::protocol("truncated DIB header"));
    }
    let width = i32::from_le_bytes(info[4..8].try_into().unwrap());
    let height_raw = i32::from_le_bytes(info[8..12].try_into().unwrap());
    let planes = u16::from_le_bytes(info[12..14].try_into().unwrap());
    let bits = u16::from_le_bytes(info[14..16].try_into().unwrap());
    let compression = u32::from_le_bytes(info[16..20].try_into().unwrap());
    if width <= 0 || planes != 1 {
        return Err(Error::protocol("unsupported BMP geometry"));
    }
    if compression != 0 {
        return Err(Error::protocol("compressed BMP is not supported"));
    }
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs();
    let w = width as u32;
    let h = height;
    let row_bytes = ((w as usize * bits as usize + 31) / 32) * 4;
    let pixels = file
        .get(pixel_off..)
        .ok_or_else(|| Error::protocol("BMP pixel offset past end of buffer"))?;
    let mut rgb = vec![0u8; w as usize * h as usize * 3];
    for y in 0..h as usize {
        let src_y = if top_down { y } else { h as usize - 1 - y };
        let start = src_y * row_bytes;
        let row = pixels
            .get(start..start + row_bytes)
            .ok_or_else(|| Error::protocol("BMP row past end of buffer"))?;
        for x in 0..w as usize {
            let dst = (y * w as usize + x) * 3;
            match bits {
                32 => {
                    let i = x * 4;
                    rgb[dst] = row[i + 2];
                    rgb[dst + 1] = row[i + 1];
                    rgb[dst + 2] = row[i];
                }
                24 => {
                    let i = x * 3;
                    rgb[dst] = row[i + 2];
                    rgb[dst + 1] = row[i + 1];
                    rgb[dst + 2] = row[i];
                }
                8 => {
                    let v = row[x];
                    rgb[dst] = v;
                    rgb[dst + 1] = v;
                    rgb[dst + 2] = v;
                }
                _ => {
                    return Err(Error::protocol(format!(
                        "unsupported BMP bit depth {bits}"
                    )))
                }
            }
        }
    }
    Ok((w, h, rgb))
}

pub fn jpeg_to_rgb(jpeg: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(jpeg));
    let pixels = decoder
        .decode()
        .map_err(|e| Error::protocol(format!("jpeg decode: {e}")))?;
    let info = decoder
        .info()
        .ok_or_else(|| Error::protocol("jpeg decode produced no header"))?;
    let w = info.width as u32;
    let h = info.height as u32;
    let rgb = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels,
        jpeg_decoder::PixelFormat::L8 => pixels
            .into_iter()
            .flat_map(|v| [v, v, v])
            .collect(),
        jpeg_decoder::PixelFormat::CMYK32 => {
            return Err(Error::protocol("CMYK JPEG is not supported"))
        }
        other => {
            return Err(Error::protocol(format!(
                "unsupported JPEG pixel format {other:?}"
            )))
        }
    };
    Ok((w, h, rgb))
}

fn threshold_rgb(rgb: &[u8]) -> Vec<u8> {
    rgb.chunks_exact(3)
        .flat_map(|p| {
            let y = (p[0] as u16 * 30 + p[1] as u16 * 59 + p[2] as u16 * 11) / 100;
            let v = if y < 128 { 0 } else { 255 };
            [v, v, v]
        })
        .collect()
}

pub fn rgb_to_tiff(rgb: &[u8], width: u32, height: u32, dpi: u32) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(Error::protocol("TIFF dimensions out of range"));
    }
    let expected = width as usize * height as usize * 3;
    if rgb.len() < expected {
        return Err(Error::protocol("RGB buffer shorter than width×height"));
    }
    let strip = &rgb[..expected];
    // Little-endian TIFF, one uncompressed RGB strip, 12 IFD entries.
    let mut t = Vec::new();
    t.extend_from_slice(b"II*\0");
    let ifd_at = 8u32;
    t.extend_from_slice(&ifd_at.to_le_bytes());
    t.extend_from_slice(&12u16.to_le_bytes());
    let bits_at = 8 + 2 + 12 * 12 + 4;
    let res_at = bits_at + 6;
    let data_at = res_at + 16;
    fn entry(tag: u16, ty: u16, count: u32, value: u32) -> Vec<u8> {
        let mut e = Vec::with_capacity(12);
        e.extend_from_slice(&tag.to_le_bytes());
        e.extend_from_slice(&ty.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&value.to_le_bytes());
        e
    }
    // SHORT=3 LONG=4 RATIONAL=5
    t.extend_from_slice(&entry(256, 4, 1, width));
    t.extend_from_slice(&entry(257, 4, 1, height));
    t.extend_from_slice(&entry(258, 3, 3, bits_at));
    t.extend_from_slice(&entry(259, 3, 1, 1));
    t.extend_from_slice(&entry(262, 3, 1, 2));
    t.extend_from_slice(&entry(273, 4, 1, data_at));
    t.extend_from_slice(&entry(277, 3, 1, 3));
    t.extend_from_slice(&entry(278, 4, 1, height));
    t.extend_from_slice(&entry(279, 4, 1, strip.len() as u32));
    t.extend_from_slice(&entry(282, 5, 1, res_at));
    t.extend_from_slice(&entry(283, 5, 1, res_at + 8));
    t.extend_from_slice(&entry(296, 3, 1, 2));
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(&8u16.to_le_bytes());
    t.extend_from_slice(&8u16.to_le_bytes());
    t.extend_from_slice(&8u16.to_le_bytes());
    let d = dpi.max(1);
    t.extend_from_slice(&d.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(&d.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(strip);
    Ok(t)
}

pub fn jpeg_to_tiff(jpeg: &[u8], dpi: u32) -> Result<Vec<u8>> {
    let (w, h, rgb) = jpeg_to_rgb(jpeg)?;
    rgb_to_tiff(&rgb, w, h, dpi)
}

pub fn apply_lineart_jpeg(jpeg: &[u8]) -> Result<Vec<u8>> {
    let (w, h, rgb) = jpeg_to_rgb(jpeg)?;
    let bw = threshold_rgb(&rgb);
    rgb_to_jpeg(&bw, w, h, 80)
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

    #[test]
    fn bmp_roundtrip_to_jpeg_and_tiff() {
        let bmp = solid_bmp_bgra(2, 2, 0, 0, 255);
        assert!(is_bmp(&bmp));
        let (w, h, rgb) = decode_bmp_or_dib(&bmp).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&rgb[0..3], &[255, 0, 0]);
        let jpeg = raster_to_jpeg(&bmp).unwrap();
        assert!(is_jpeg(&jpeg));
        let tiff = rgb_to_tiff(&rgb, w, h, 300).unwrap();
        assert!(is_tiff(&tiff));
    }
}
