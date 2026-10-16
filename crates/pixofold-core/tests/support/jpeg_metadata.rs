//! 确定性JPEG元数据语料；C2PA只模拟容器，不声称具有有效签名。

fn boxed(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    [(data.len() as u32 + 8).to_be_bytes().as_slice(), kind, data].concat()
}
fn exif() -> Vec<u8> {
    // IFD0: Orientation=6、相机名、Exif/GPS子IFD；Exif含日期与不透明MakerNote。
    let mut tiff = vec![0u8; 160];
    tiff[..8].copy_from_slice(b"II\x2a\0\x08\0\0\0");
    let u16_at = |b: &mut [u8], at, n: u16| b[at..at + 2].copy_from_slice(&n.to_le_bytes());
    let u32_at = |b: &mut [u8], at, n: u32| b[at..at + 4].copy_from_slice(&n.to_le_bytes());
    u16_at(&mut tiff, 8, 4);
    for (at, tag, kind, count, value) in [
        (10, 0x0112, 3, 1, 6),
        (22, 0x010f, 2, 6, 62),
        (34, 0x8769, 4, 1, 68),
        (46, 0x8825, 4, 1, 118),
        (70, 0x9003, 2, 20, 98),
        (82, 0x927c, 7, 4, 0x12345678),
        (120, 0, 1, 4, 0x00000302),
        (132, 1, 2, 2, 0x4e),
    ] {
        u16_at(&mut tiff, at, tag);
        u16_at(&mut tiff, at + 2, kind);
        u32_at(&mut tiff, at + 4, count);
        u32_at(&mut tiff, at + 8, value);
    }
    tiff[62..68].copy_from_slice(b"Pixo!\0");
    u16_at(&mut tiff, 68, 2);
    tiff[98..118].copy_from_slice(b"2026:10:15 10:00:00\0");
    u16_at(&mut tiff, 118, 2);
    [b"Exif\0\0".as_slice(), &tiff].concat()
}
fn xmp() -> Vec<u8> {
    [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), br#"<?xpacket begin=''?><x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><rdf:Description xmlns:dc='http://purl.org/dc/elements/1.1/' dc:format='image/jpeg'><dc:title><rdf:Alt><rdf:li xml:lang='x-default'>PixoFold fixture &amp; metadata</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>"#].concat()
}
fn photoshop() -> Vec<u8> {
    let iptc = b"\x1c\x02\x78\0\x04test";
    let mut result = [
        b"Photoshop 3.0\08BIM\x04\x04\0\0".as_slice(),
        &(iptc.len() as u32).to_be_bytes(),
        iptc,
    ]
    .concat();
    if !iptc.len().is_multiple_of(2) {
        result.push(0);
    }
    result
}
fn credential_segments() -> Vec<(u8, Vec<u8>)> {
    let suffix = b"\0\x11\0\x10\x80\0\0\xaa\0\x38\x9b\x71";
    let root = boxed(
        b"jumd",
        &[b"c2pa".as_slice(), suffix, b"\x03c2pa\0"].concat(),
    );
    let manifest = boxed(
        b"jumb",
        &[
            boxed(
                b"jumd",
                &[b"c2ma".as_slice(), suffix, b"\x03urn:test\0"].concat(),
            ),
            boxed(b"cbor", b"\xa0"),
        ]
        .concat(),
    );
    let store = boxed(b"jumb", &[root, manifest].concat());
    let split = 45;
    [&store[8..split], &store[split..]]
        .into_iter()
        .enumerate()
        .map(|(i, data)| {
            (
                0xeb,
                [
                    b"JP\x02\x11".as_slice(),
                    &(i as u32 + 1).to_be_bytes(),
                    &store[..8],
                    data,
                ]
                .concat(),
            )
        })
        .collect()
}
fn insert(base: &[u8], segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
    assert!(base.starts_with(&[0xff, 0xd8]));
    let mut bytes = base[..2].to_vec();
    for (marker, data) in segments {
        bytes.extend_from_slice(&[0xff, *marker]);
        bytes.extend_from_slice(&u16::try_from(data.len() + 2).unwrap().to_be_bytes());
        bytes.extend_from_slice(data);
    }
    bytes.extend_from_slice(&base[2..]);
    bytes
}

/// 返回普通元数据、相同元数据+C2PA、坏C2PA及未知APP11；原始JPEG熵数据不变。
pub fn samples(base: &[u8]) -> [Vec<u8>; 4] {
    let segments = vec![(0xe1, exif()), (0xe1, xmp()), (0xed, photoshop())];
    let ordinary = insert(base, &segments);
    let signed = insert(&ordinary, &credential_segments());
    let mut broken = credential_segments();
    broken[1].1[7] = 3;
    let broken = insert(&ordinary, &broken);
    let unknown = insert(&ordinary, &[(0xeb, b"unrecognized APP11".to_vec())]);
    [ordinary, signed, broken, unknown]
}
