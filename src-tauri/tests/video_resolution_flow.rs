use local_film_library_preview_lib::video_metadata;
use std::fs;
fn chunk(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = Vec::from(*name);
    v.extend((data.len() as u32).to_le_bytes());
    v.extend(data);
    if data.len() % 2 != 0 {
        v.push(0);
    }
    v
}
#[cfg(windows)]
#[test]
fn windows_reads_resolution_from_synthetic_avi_without_writing_it() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("synthetic.avi");
    let mut avih = vec![0u8; 56];
    avih[0..4].copy_from_slice(&40000u32.to_le_bytes());
    avih[16..20].copy_from_slice(&1u32.to_le_bytes());
    avih[24..28].copy_from_slice(&1u32.to_le_bytes());
    avih[32..36].copy_from_slice(&320u32.to_le_bytes());
    avih[36..40].copy_from_slice(&240u32.to_le_bytes());
    let mut strh = vec![0u8; 56];
    strh[0..4].copy_from_slice(b"vids");
    strh[4..8].copy_from_slice(b"DIB ");
    strh[20..24].copy_from_slice(&1u32.to_le_bytes());
    strh[24..28].copy_from_slice(&25u32.to_le_bytes());
    strh[32..36].copy_from_slice(&1u32.to_le_bytes());
    strh[48..52].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut bitmap = vec![0u8; 40];
    bitmap[0..4].copy_from_slice(&40u32.to_le_bytes());
    bitmap[4..8].copy_from_slice(&320i32.to_le_bytes());
    bitmap[8..12].copy_from_slice(&240i32.to_le_bytes());
    bitmap[12..14].copy_from_slice(&1u16.to_le_bytes());
    bitmap[14..16].copy_from_slice(&24u16.to_le_bytes());
    bitmap[20..24].copy_from_slice(&230400u32.to_le_bytes());
    let mut stream = b"strl".to_vec();
    stream.extend(chunk(b"strh", &strh));
    stream.extend(chunk(b"strf", &bitmap));
    let mut header = b"hdrl".to_vec();
    header.extend(chunk(b"avih", &avih));
    header.extend(chunk(b"LIST", &stream));
    let mut movie = b"movi".to_vec();
    movie.extend(chunk(b"00db", &vec![0u8; 230400]));
    let mut avi = b"AVI ".to_vec();
    avi.extend(chunk(b"LIST", &header));
    avi.extend(chunk(b"LIST", &movie));
    let bytes = chunk(b"RIFF", &avi);
    fs::write(&file, &bytes).unwrap();
    assert_eq!(
        video_metadata::dimensions(&file).map(|v| (v.width, v.height)),
        Some((320, 240))
    );
    assert_eq!(video_metadata::dimensions(&file.canonicalize().unwrap()).map(|v| (v.width, v.height)), Some((320, 240)), "actual playback paths are canonicalized on Windows");
    assert_eq!(fs::read(&file).unwrap(), bytes);
}
#[test]
fn unavailable_metadata_is_optional_and_original_bytes_are_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("invalid.mp4");
    fs::write(&file, b"synthetic invalid video").unwrap();
    assert!(video_metadata::dimensions(&file).is_none());
    assert_eq!(fs::read(&file).unwrap(), b"synthetic invalid video");
    assert!(video_metadata::dimensions(&temp.path().join("missing.mp4")).is_none());
}
