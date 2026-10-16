use super::*;

fn box_bytes(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    [
        (payload.len() as u32 + 8).to_be_bytes().as_slice(),
        kind,
        payload,
    ]
    .concat()
}
fn store() -> Vec<u8> {
    let uuid = b"c2pa\0\x11\0\x10\x80\0\0\xaa\0\x38\x9b\x71";
    let description = box_bytes(b"jumd", &[uuid.as_slice(), b"\x03c2pa\0"].concat());
    let manifest = box_bytes(
        b"jumb",
        &[
            box_bytes(
                b"jumd",
                &[b"c2ma".as_slice(), &uuid[4..], b"\x03urn:test\0"].concat(),
            ),
            box_bytes(b"cbor", b"\xa0"),
        ]
        .concat(),
    );
    box_bytes(b"jumb", &[description, manifest].concat())
}
fn part(store: &[u8], sequence: u32, payload: &[u8]) -> Vec<u8> {
    [
        b"JP\x02\x11".as_slice(),
        &sequence.to_be_bytes(),
        &store[..8],
        payload,
    ]
    .concat()
}

#[test]
fn recognizes_only_complete_c2pa_and_removes_exact_segment_ranges() {
    let bytes = store();
    let first = part(&bytes, 1, &bytes[8..40]);
    let last = part(&bytes, 2, &bytes[40..]);
    let ranges = [
        2..2 + first.len() + 4,
        2 + first.len() + 4..2 + first.len() + last.len() + 8,
    ];
    let metadata = [(0xeb, first.as_slice()), (0xeb, last.as_slice())];
    let info = inspect(&metadata, &ranges).unwrap();
    assert_eq!(info.credentials, ranges);
    let mut image = vec![1, 2];
    image.resize(ranges[1].end, 9);
    image.extend_from_slice(&[3, 4, 5]);
    assert_eq!(remove(&image, &info.credentials).unwrap(), [1, 2, 3, 4, 5]);
    assert!(inspect(&metadata[..1], &ranges[..1]).is_err());
    let bad = part(&bytes, 3, &bytes[40..]);
    assert!(inspect(&[(0xeb, &first), (0xeb, &bad)], &ranges).is_err());
    let mut unrelated = bytes.clone();
    unrelated[24] = b'x';
    let unrelated = part(&unrelated, 1, &unrelated[8..]);
    assert!(
        inspect(
            &[(0xeb, &unrelated)],
            std::slice::from_ref(&(0..unrelated.len()))
        )
        .is_err()
    );
    let separated = [ranges[0].clone(), ranges[1].start + 1..ranges[1].end + 1];
    assert!(inspect(&metadata, &separated).is_err());
}

#[test]
fn xmp_preserves_ordinary_rdf_but_rejects_external_credentials_dtd_and_bad_xml() {
    let ordinary = br#"<?xpacket begin=''?><x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><rdf:Description xmlns:t='http://ns.adobe.com/tiff/1.0/' t:Make='Camera &amp; Lens'/></rdf:RDF></x:xmpmeta><?xpacket end='w'?>"#;
    assert!(xmp::inspect(ordinary).is_ok());
    for invalid in [
        "<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><Description xmlns:p='http://purl.org/dc/terms/' p:provenance='https://example.test/manifest'/></RDF>",
        "<!DOCTYPE a [<!ENTITY x SYSTEM 'file:///test'>]><RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'/>",
        "<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><a></RDF>",
        "<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'/><extra/>",
        "<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>&unknown;</RDF>",
        "<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>&#0;</RDF>",
        "<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#' name='&#xfffe;'/>",
        "<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#' xmlns:a='urn:test' xmlns:b='urn:test' a:x='1' b:x='2'/>",
    ] {
        assert!(xmp::inspect(invalid.as_bytes()).is_err(), "{invalid}");
    }
}

fn exif(little: bool) -> Vec<u8> {
    let mut tiff = vec![0u8; 106];
    tiff[..2].copy_from_slice(if little { b"II" } else { b"MM" });
    let u16_at = |bytes: &mut [u8], at, n: u16| {
        bytes[at..at + 2].copy_from_slice(&if little {
            n.to_le_bytes()
        } else {
            n.to_be_bytes()
        });
    };
    let u32_at = |bytes: &mut [u8], at, n: u32| {
        bytes[at..at + 4].copy_from_slice(&if little {
            n.to_le_bytes()
        } else {
            n.to_be_bytes()
        });
    };
    u16_at(&mut tiff, 2, 42);
    u32_at(&mut tiff, 4, 8);
    u16_at(&mut tiff, 8, 3);
    for (entry, tag, kind, count, value) in [
        (10, 0x0112, 3, 1, 6),
        (22, 0x010f, 2, 6, 50),
        (34, 0x8769, 4, 1, 56),
        (58, 0x9003, 2, 20, 86),
        (70, 0x927c, 7, 4, 0x12345678),
    ] {
        u16_at(&mut tiff, entry, tag);
        u16_at(&mut tiff, entry + 2, kind);
        u32_at(&mut tiff, entry + 4, count);
        if kind == 3 {
            u16_at(&mut tiff, entry + 8, value as u16);
        } else {
            u32_at(&mut tiff, entry + 8, value);
        }
    }
    tiff[50..56].copy_from_slice(b"Pixo!\0");
    u16_at(&mut tiff, 56, 2);
    tiff[86..].copy_from_slice(b"2026:10:15 10:00:00\0");
    [b"Exif\0\0".as_slice(), &tiff].concat()
}

#[test]
fn exif_accepts_both_orders_and_nested_metadata_without_modifying_bytes() {
    for little in [false, true] {
        let source = exif(little);
        assert_eq!(super::exif::inspect(&source).unwrap(), 6);
        for end in 0..source.len() {
            assert!(super::exif::inspect(&source[..end]).is_err());
        }
        let mut cycle = source.clone();
        cycle[48..52].copy_from_slice(&if little {
            8u32.to_le_bytes()
        } else {
            8u32.to_be_bytes()
        });
        assert!(super::exif::inspect(&cycle).is_err());
        let mut overflow = source.clone();
        overflow[36..40].fill(0xff);
        assert!(super::exif::inspect(&overflow).is_err());
    }
}

#[test]
fn exif_and_xmp_can_coexist_while_duplicates_and_unknown_app11_are_rejected() {
    let exif = exif(true);
    let xmp = [
        xmp::PREFIX,
        br#"<rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'/>"#,
    ]
    .concat();
    assert!(
        inspect(
            &[(0xe1, &exif), (0xe1, &xmp)],
            &[0..exif.len(), exif.len()..exif.len() + xmp.len()]
        )
        .is_ok()
    );
    assert!(
        inspect(
            &[(0xe1, &exif), (0xe1, &exif)],
            &[0..exif.len(), 0..exif.len()]
        )
        .is_err()
    );
    assert!(inspect(&[(0xeb, b"unknown")], std::slice::from_ref(&(0..7))).is_err());
}

#[test]
fn photoshop_embedded_xmp_cannot_hide_credentials_or_bad_resource_lengths() {
    let document = b"<RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#' xmlns:p='http://purl.org/dc/terms/' p:provenance='remote'/>";
    let mut bytes = [
        b"Photoshop 3.0\08BIM\x04\x24\0\0".as_slice(),
        &(document.len() as u32).to_be_bytes(),
        document,
    ]
    .concat();
    if !document.len().is_multiple_of(2) {
        bytes.push(0);
    }
    assert!(matches!(
        photoshop(&bytes),
        Err(JpegError::ProtectedMetadata(0xe1))
    ));
    bytes[21..25].fill(0xff);
    assert!(photoshop(&bytes).is_err());
}
