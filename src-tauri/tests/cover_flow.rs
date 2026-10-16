use local_film_library_preview_lib::{
    catalog, covers, observation::scan_into_library, storage::Storage,
};
use std::fs;

fn setup() -> (tempfile::TempDir, Storage, String) {
    let home = tempfile::tempdir().unwrap();
    let media = home.path().join("media");
    fs::create_dir_all(media.join("演员/[DEMO-01] 虚构影片")).unwrap();
    fs::write(media.join("演员/[DEMO-01] 虚构影片/a.mp4"), []).unwrap();
    png(&media.join("演员/actor.jpg"), 10);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构来源',?1)",
                [media.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    scan_into_library(&storage, "source", &media).unwrap();
    let id = catalog::list_films(&storage).unwrap()[0].id.clone();
    (home, storage, id)
}

fn png(path: &std::path::Path, color: u8) {
    let img = image::RgbImage::from_pixel(40, 60, image::Rgb([color, 20, 30]));
    img.save_with_format(path, image::ImageFormat::Png).unwrap();
}

#[test]
fn explicit_cover_decodes_actual_content_and_cache_updates_and_rebuilds() {
    let (home, storage, id) = setup();
    let media = home.path().join("media");
    let original = media.join("演员/[DEMO-01] 虚构影片/poster.jpg");
    png(&original, 80); // Deliberately PNG content with JPEG extension.
    png(&media.join("演员/poster.jpg"), 180);
    scan_into_library(&storage, "source", &media).unwrap();
    let info = covers::info(&storage, &id).unwrap();
    assert_eq!(info.candidates.len(), 1);
    assert_eq!(info.default_id, Some(info.candidates[0].id.clone()));
    let first = covers::cached_thumbnail(&storage, &id, false).unwrap();
    assert!(image::load_from_memory(&first).is_ok());
    assert_eq!(
        fs::read(&original).unwrap()[..8],
        [137, 80, 78, 71, 13, 10, 26, 10]
    );
    let original_size = fs::metadata(&original).unwrap().len();
    png(&original, 120);
    assert_eq!(fs::metadata(&original).unwrap().len(), original_size);
    let scan = local_film_library_preview_lib::source_scan::SourceScanSession::new();
    let token = scan.preview(&storage, &media, Some("source")).unwrap().token.unwrap();
    local_film_library_preview_lib::lifecycle::confirm_source_refresh(&storage, &scan, &token).unwrap();
    let second = covers::cached_thumbnail(&storage, &id, false).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        covers::cached_thumbnail(&storage, &id, false).unwrap(),
        second
    );
    fs::remove_dir_all(home.path().join("data/cover-cache")).unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, &id, false).unwrap(),
        second
    );
    for file in fs::read_dir(home.path().join("data/cover-cache")).unwrap() {
        fs::write(file.unwrap().path(), b"broken generated cache").unwrap();
    }
    assert!(covers::cached_thumbnail(&storage, &id, false).is_err());
    local_film_library_preview_lib::lifecycle::clear_generated_cache(&storage).unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, &id, false).unwrap(),
        second
    );
}

#[test]
fn jpeg_png_webp_content_is_supported_and_non_images_bad_and_large_inputs_keep_film() {
    let (home, storage, id) = setup();
    let media = home.path().join("media");
    let path = media.join("演员/[DEMO-01] 虚构影片/poster.jpg");
    let image = image::RgbImage::from_pixel(32, 48, image::Rgb([100, 30, 20]));
    for format in [
        image::ImageFormat::Jpeg,
        image::ImageFormat::Png,
        image::ImageFormat::WebP,
    ] {
        image.save_with_format(&path, format).unwrap();
        scan_into_library(&storage, "source", &media).unwrap();
        local_film_library_preview_lib::lifecycle::clear_generated_cache(&storage).unwrap();
        assert!(covers::cached_thumbnail(&storage, &id, true).is_ok());
    }

    for bad in [
        b"broken".as_slice(),
        b"<svg xmlns='http://www.w3.org/2000/svg'/>".as_slice(),
    ] {
        fs::write(&path, bad).unwrap();
        local_film_library_preview_lib::lifecycle::clear_generated_cache(&storage).unwrap();
        assert_eq!(
            covers::cached_thumbnail(&storage, &id, false).unwrap_err(),
            "cover_invalid"
        );
    }
    fs::File::create(&path)
        .unwrap()
        .set_len(33 * 1024 * 1024)
        .unwrap();
    assert_eq!(
        covers::cached_thumbnail(&storage, &id, false).unwrap_err(),
        "cover_too_large"
    );
    image::RgbImage::new(12001, 1)
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    assert!(covers::cached_thumbnail(&storage, &id, false).is_err());
    assert_eq!(catalog::list_films(&storage).unwrap().len(), 1);
}

#[test]
fn direct_fixed_cover_does_not_borrow_into_other_films() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("synthetic-source");
    fs::create_dir_all(root.join("[DEMO-99] 直属封面")).unwrap();
    fs::create_dir_all(root.join("[DEMO-01] 影片")).unwrap();
    fs::write(root.join("[DEMO-99] 直属封面/direct.mp4"), []).unwrap();
    fs::write(root.join("[DEMO-01] 影片/a.mp4"), []).unwrap();
    png(&root.join("poster.jpg"), 30);
    png(&root.join("[DEMO-99] 直属封面/poster.jpg"), 80);
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage
        .with_connection(|db| {
            db.execute(
                "INSERT INTO libraries(id,display_name,root_path) VALUES('source','虚构容器',?1)",
                [root.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
        .unwrap();
    scan_into_library(&storage, "source", &root).unwrap();
    let films = catalog::list_films(&storage).unwrap();
    assert_eq!(films.len(), 2);
    for film in films {
        let info = covers::info(&storage, &film.id).unwrap();
        assert_eq!(
            info.candidates.len(),
            usize::from(film.videos[0].observed_path.ends_with("/direct.mp4"))
        );
    }
}

#[test]
fn redirected_original_directory_and_cache_are_rejected() {
    for redirect_cache in [false, true] {
        let (home, storage, id) = setup();
        let media = home.path().join("media");
        let folder = media.join("演员/[DEMO-01] 虚构影片");
        png(&folder.join("poster.jpg"), 40);
        scan_into_library(&storage, "source", &media).unwrap();

        covers::cached_thumbnail(&storage, &id, false).unwrap();
        if !redirect_cache {
            local_film_library_preview_lib::lifecycle::clear_generated_cache(&storage).unwrap();
        }
        let link = if redirect_cache {
            home.path().join("data/cover-cache")
        } else {
            folder
        };
        let outside = home.path().join("isolated-redirect");
        fs::rename(&link, &outside).unwrap();
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(outside.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            covers::cached_thumbnail(&storage, &id, false).unwrap_err(),
            if redirect_cache {
                "cover_cache_failed"
            } else {
                "cover_not_owned"
            }
        );
        assert!(!fs::read_dir(&outside)
            .unwrap()
            .collect::<Vec<_>>()
            .is_empty());
    }
}
