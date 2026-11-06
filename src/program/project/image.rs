//! What a picture file says of itself in its first bytes: which of the three formats the terminal
//! draws it is (PNG, JPEG, SVG) and how many pixels wide and tall. Nothing is decoded and no
//! library is needed for it: the size is in the header, and a file whose header says none is not a
//! picture here.
//!
//! The terminal decides the format by the bytes and not by the name, and so does this; the name
//! only tells which files are worth the reading.

/// The most bytes a picture file may be: the terminal draws no larger file.
pub const BYTES_LIMIT: u64 = 8 * 1024 * 1024;

/// The most pixels a picture may be wide or tall: the terminal draws no larger one.
pub const SIDE_LIMIT: u32 = 4096;

/// How many bytes of an SVG are looked at for its root element.
const SVG_HEAD: usize = 4096;

/// What a picture file is, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Whether the name of a file is one of a picture the terminal draws: `.png`, `.jpg`, `.jpeg` or
/// `.svg`, in any case.
pub fn is_named_so(name: &str) -> bool {
    let Some((_, extension)) = name.rsplit_once('.') else {
        return false;
    };
    ["png", "jpg", "jpeg", "svg"]
        .iter()
        .any(|known| extension.eq_ignore_ascii_case(known))
}

/// The size of the picture in `bytes`, as the terminal shows it (a photograph turned by its EXIF
/// orientation is as wide as it is shown); none for bytes that are no picture of the three formats
/// or whose header says no size.
pub fn size_of(bytes: &[u8]) -> Option<Size> {
    let size = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        jpeg(bytes)
    } else {
        svg(bytes)
    }?;
    (size.width > 0 && size.height > 0).then_some(size)
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let four: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_be_bytes(four))
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    let two: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(u32::from(u16::from_be_bytes(two)))
}

/// The `IHDR` chunk comes first, after the signature: its length, its name, the width and the height.
fn png(bytes: &[u8]) -> Option<Size> {
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    Some(Size {
        width: be32(bytes, 16)?,
        height: be32(bytes, 20)?,
    })
}

/// The first frame header (`SOF0` to `SOF15`, but for the three that are no frames) has the height
/// and the width. The segments before it are skipped by their lengths; the EXIF block among them
/// says how the photograph was held.
fn jpeg(bytes: &[u8]) -> Option<Size> {
    let mut at = 2;
    let mut turned = false;
    loop {
        // Segments begin with 0xFF, which fill bytes may be repeated before.
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        while *bytes.get(at)? == 0xFF {
            at += 1;
        }
        let marker = *bytes.get(at)?;
        at += 1;
        match marker {
            // Standalone: restarts, the start and the end of the image, a temporary marker.
            0x00 | 0x01 | 0xD0..=0xD8 => continue,
            // The end of the image, or the start of its scan: no frame header came.
            0xD9 | 0xDA => return None,
            _ => {}
        }
        let length = usize::try_from(be16(bytes, at)?).ok()?;
        if length < 2 {
            return None;
        }
        let body = bytes.get(at + 2..at.checked_add(length)?)?;
        match marker {
            0xE1 if body.starts_with(b"Exif\0\0") => {
                turned = body.get(6..).is_some_and(turns_a_quarter);
            }
            // Not frames: the Huffman tables, the JPEG extensions, the arithmetic conditioning.
            0xC4 | 0xC8 | 0xCC => {}
            0xC0..=0xCF => {
                let (height, width) = (be16(body, 1)?, be16(body, 3)?);
                return Some(if turned {
                    Size {
                        width: height,
                        height: width,
                    }
                } else {
                    Size { width, height }
                });
            }
            _ => {}
        }
        at += length;
    }
}

/// Whether the EXIF block `tiff` (from its TIFF header on) says the photograph is to be turned a
/// quarter, or mirrored along a diagonal: orientations 5 to 8, which swap the width and the height.
fn turns_a_quarter(tiff: &[u8]) -> bool {
    let little = match tiff.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return false,
    };
    let short = |at: usize| -> Option<u16> {
        let two: [u8; 2] = tiff.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(two)
        } else {
            u16::from_be_bytes(two)
        })
    };
    let long = |at: usize| -> Option<u32> {
        let four: [u8; 4] = tiff.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(four)
        } else {
            u32::from_be_bytes(four)
        })
    };
    let orientation = || -> Option<u16> {
        let directory = usize::try_from(long(4)?).ok()?;
        let entries = usize::from(short(directory)?);
        (0..entries).find_map(|n| {
            let entry = directory.checked_add(2)?.checked_add(n.checked_mul(12)?)?;
            if short(entry)? != 0x0112 {
                return None;
            }
            short(entry.checked_add(8)?)
        })
    };
    matches!(orientation(), Some(5..=8))
}

/// An SVG says its size by the `width` and `height` of its root element, or, where one is missing or
/// is a part of its box (`50%`), by its `viewBox`: the one is the other's proportions.
fn svg(bytes: &[u8]) -> Option<Size> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let head = String::from_utf8_lossy(bytes.get(..bytes.len().min(SVG_HEAD)).unwrap_or_default());
    let tag = root_tag(&head)?;
    let width = attribute(tag, "width").and_then(|value| pixels(&value));
    let height = attribute(tag, "height").and_then(|value| pixels(&value));
    let view = attribute(tag, "viewBox").and_then(|value| view_box(&value));
    let (width, height) = match (width, height, view) {
        (Some(width), Some(height), _) => (width, height),
        (Some(width), None, Some((w, h))) => (width, width * h / w),
        (None, Some(height), Some((w, h))) => (height * w / h, height),
        (None, None, Some((w, h))) => (w, h),
        _ => return None,
    };
    Some(Size {
        width: whole(width)?,
        height: whole(height)?,
    })
}

/// A length in pixels, rounded up and not past what a number of pixels is.
fn whole(length: f64) -> Option<u32> {
    if length.is_finite() && length > 0.0 && length <= f64::from(u32::MAX) {
        // The range is checked above.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(length.ceil() as u32)
    } else {
        None
    }
}

/// The text of the `<svg ...>` start tag, after a declaration, a doctype, comments and the
/// instructions that may come before it.
fn root_tag(head: &str) -> Option<&str> {
    let mut rest = head.trim_start();
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.split_once("-->")?.1;
        } else if let Some(after) = rest.strip_prefix("<?") {
            rest = after.split_once("?>")?.1;
        } else if rest.starts_with("<!") {
            // A doctype may hold a subset in brackets, with `>` in it.
            let end = match (rest.find('['), rest.find('>')) {
                (Some(open), Some(close)) if open < close => rest.find("]>").map(|at| at + 1)?,
                (_, Some(close)) => close,
                _ => return None,
            };
            rest = rest.get(end + 1..)?;
        } else {
            break;
        }
    }
    let after = rest.strip_prefix("<svg")?;
    // The name ends at a space, `/` or `>`: `<svgfoo>` is no svg.
    if !after.starts_with(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/') {
        return None;
    }
    // The tag ends at the first `>` outside a quoted value.
    let mut quote = None;
    for (at, ch) in after.char_indices() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(open), _) if open == ch => quote = None,
            (None, '>') => return after.get(..at),
            _ => {}
        }
    }
    None
}

/// The value of the attribute `name` in the text of a start tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest.get(..at).and_then(|text| text.chars().next_back());
        let after = rest.get(at + name.len()..)?;
        let boundary = before.is_none_or(|ch| ch.is_whitespace() || ch == '"' || ch == '\'');
        let after = after.trim_start();
        if boundary && let Some(value) = after.strip_prefix('=') {
            let value = value.trim_start();
            let quote = value
                .chars()
                .next()
                .filter(|ch| *ch == '"' || *ch == '\'')?;
            let inside = value.get(1..)?;
            return inside.split_once(quote).map(|(text, _)| text.to_owned());
        }
        rest = rest.get(at + name.len()..)?;
    }
    None
}

/// A length in pixels (96 to the inch): a number, or one with the unit of the SVG that has one.
/// A part of a box (`%`) and the units of a font are none.
fn pixels(value: &str) -> Option<f64> {
    let value = value.trim();
    let end = value
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(end);
    let number: f64 = number.parse().ok()?;
    let scale = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "px" => 1.0,
        "pt" => 4.0 / 3.0,
        "pc" => 16.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "in" => 96.0,
        _ => return None,
    };
    (number > 0.0).then_some(number * scale)
}

/// The width and the height of a `viewBox`, a list of four numbers.
fn view_box(value: &str) -> Option<(f64, f64)> {
    let numbers: Vec<f64> = value
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    match numbers[..] {
        [_, _, width, height] if width > 0.0 && height > 0.0 => Some((width, height)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG header for a picture of `width` by `height`, as far as the size goes.
    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    /// A JPEG with the segments given before its frame header, which says `width` by `height`.
    fn jpeg_of(width: u16, height: u16, before: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        for segment in before {
            bytes.extend(segment);
        }
        bytes.extend([0xFF, 0xC0, 0, 17, 8]);
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        bytes
    }

    /// An EXIF segment that says the orientation `turn`.
    fn exif_of(turn: u16, little: bool) -> Vec<u8> {
        let short = |value: u16| {
            if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            }
        };
        let mut tiff = if little {
            b"II*\0".to_vec()
        } else {
            b"MM\0*".to_vec()
        };
        tiff.extend(if little {
            8_u32.to_le_bytes()
        } else {
            8_u32.to_be_bytes()
        });
        tiff.extend(short(1));
        tiff.extend(short(0x0112));
        tiff.extend(short(3));
        tiff.extend(if little {
            1_u32.to_le_bytes()
        } else {
            1_u32.to_be_bytes()
        });
        tiff.extend(short(turn));
        tiff.extend([0, 0]);
        let mut body = b"Exif\0\0".to_vec();
        body.extend(tiff);
        let mut segment = vec![0xFF, 0xE1];
        segment.extend(u16::try_from(body.len() + 2).unwrap().to_be_bytes());
        segment.extend(body);
        segment
    }

    fn size(width: u32, height: u32) -> Option<Size> {
        Some(Size { width, height })
    }

    #[test]
    fn the_name_tells_which_files_are_worth_reading() {
        for name in ["a.png", "A.PNG", "x.y.jpg", "p.JPEG", "logo.svg"] {
            assert!(is_named_so(name), "{name}");
        }
        for name in ["a.gif", "png", "a.png.txt", "a.webp", "", ".png.bak"] {
            assert!(!is_named_so(name), "{name}");
        }
    }

    #[test]
    fn a_png_says_its_size_in_its_first_chunk() {
        assert_eq!(size_of(&png_of(640, 480)), size(640, 480));
        assert_eq!(size_of(&png_of(1, 1)), size(1, 1));
        // A picture with no pixels is none; a header cut short, or whose first chunk is another, too.
        assert_eq!(size_of(&png_of(0, 5)), None);
        assert_eq!(size_of(&png_of(5, 0)), None);
        assert_eq!(size_of(&png_of(9, 9)[..20]), None);
        let mut other = png_of(9, 9);
        other[12..16].copy_from_slice(b"IDAT");
        assert_eq!(size_of(&other), None);
    }

    #[test]
    fn a_jpeg_says_its_size_in_its_frame_header_past_the_segments_before_it() {
        assert_eq!(size_of(&jpeg_of(800, 600, &[])), size(800, 600));
        // A comment and an application segment before it are skipped by their lengths, and so are
        // the tables, whose marker looks like a frame's.
        let comment = vec![0xFF, 0xFE, 0, 5, b'h', b'i', b'!'];
        let tables = vec![0xFF, 0xC4, 0, 4, 0, 0];
        assert_eq!(size_of(&jpeg_of(31, 17, &[comment, tables])), size(31, 17));
        // Cut short, or ending before any frame came.
        assert_eq!(size_of(&jpeg_of(8, 8, &[])[..10]), None);
        assert_eq!(size_of(&[0xFF, 0xD8, 0xFF, 0xD9]), None);
        assert_eq!(size_of(&[0xFF, 0xD8, 0xFF, 0xDA, 0, 2]), None);
    }

    #[test]
    fn a_photograph_held_on_its_side_is_as_wide_as_it_is_tall_to_be_shown() {
        for little in [true, false] {
            for (turn, wide) in [(1, 800), (3, 800), (6, 600), (8, 600), (5, 600), (7, 600)] {
                let jpeg = jpeg_of(800, 600, &[exif_of(turn, little)]);
                assert_eq!(
                    size_of(&jpeg).map(|size| size.width),
                    Some(wide),
                    "orientation {turn}, little endian {little}"
                );
            }
        }
        // An EXIF block that is not one is no turn.
        let broken = vec![0xFF, 0xE1, 0, 8, b'E', b'x', b'i', b'f', 0, 0];
        assert_eq!(size_of(&jpeg_of(800, 600, &[broken])), size(800, 600));
    }

    #[test]
    fn an_svg_says_its_size_by_its_width_and_height_or_its_view_box() {
        let size_of = |text: &str| size_of(text.as_bytes());
        assert_eq!(
            size_of(r#"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"/>"#),
            size(120, 80)
        );
        assert_eq!(
            size_of(r#"<svg width='24px' height='12px'></svg>"#),
            size(24, 12)
        );
        assert_eq!(
            size_of(r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path/></svg>"#),
            size(24, 24)
        );
        assert_eq!(
            size_of(r#"<svg viewBox="0,0,48.5,10"></svg>"#),
            size(49, 10)
        );
        // One side and the box: the other keeps the box's proportions.
        assert_eq!(
            size_of(r#"<svg width="100" viewBox="0 0 50 25"></svg>"#),
            size(100, 50)
        );
        assert_eq!(
            size_of(r#"<svg height="100" viewBox="0 0 50 25"></svg>"#),
            size(200, 100)
        );
        // A part of a box is no size; the viewBox is.
        assert_eq!(
            size_of(r#"<svg width="100%" height="100%" viewBox="0 0 30 20"></svg>"#),
            size(30, 20)
        );
        // Units: 96 to the inch.
        assert_eq!(
            size_of(r#"<svg width="1in" height="0.5in"></svg>"#),
            size(96, 48)
        );
        assert_eq!(
            size_of(r#"<svg width="72pt" height="36pt"></svg>"#),
            size(96, 48)
        );
        // No size at all, and no svg.
        assert_eq!(
            size_of(r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#),
            None
        );
        assert_eq!(size_of(r"<svgfoo width='5' height='5'></svgfoo>"), None);
        assert_eq!(size_of("<html><svg width='5' height='5'/></html>"), None);
        assert_eq!(size_of("just words"), None);
        assert_eq!(size_of(""), None);
    }

    #[test]
    fn an_svg_after_its_declaration_comments_and_doctype_is_read_all_the_same() {
        let text = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <!-- <svg width=\"1\" height=\"1\"> -->\n\
            <!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\" [ <!ENTITY a \"b\"> ]>\n\
            <svg\n  xmlns=\"http://www.w3.org/2000/svg\"\n  width=\"64\"\n  height=\"32\">\n</svg>";
        assert_eq!(size_of(text.as_bytes()), size(64, 32));
        // A `>` inside a value does not end the tag; an attribute of a longer name is not the one.
        let quoted = r#"<svg data-title="a > b" data-width="9" width="7" height="3"></svg>"#;
        assert_eq!(size_of(quoted.as_bytes()), size(7, 3));
    }
}
