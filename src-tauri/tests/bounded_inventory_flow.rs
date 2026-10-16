use local_film_library_preview_lib::{inventory::inventory, directory_roles::{self, Role}, storage::Storage};
use std::{fs, path::Path};

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}

#[test]
fn source_and_nested_video_directories_have_independent_read_sets() {
    let home = tempfile::tempdir().unwrap();
    for file in ["root.mp4", "film/main.mp4", "film/subtitle.srt", "film/photos/extra.mp4", "film/photos/actor.jpg", "film/photos/poster.jpg"] {
        touch(home.path(), file);
    }
    let report = inventory(home.path()).unwrap();
    assert_eq!(report.videos.iter().map(|v| v.relative_path.as_str()).collect::<Vec<_>>(), ["film/main.mp4", "film/photos/extra.mp4", "root.mp4"]);
    assert_eq!(report.assets.iter().map(|a| a.relative_path.as_str()).collect::<Vec<_>>(), ["film/photos/actor.jpg", "film/photos/poster.jpg", "film/subtitle.srt"]);
    assert_eq!(report.cover_checks.len(),1);
    assert!(report.directories.iter().any(|p| p == "film/photos"));
    assert!(report.issues.is_empty());
}

#[test]
fn actor_marker_is_an_ordinary_image_in_recursive_video_directories() {
    let home = tempfile::tempdir().unwrap();
    for file in ["actor/actor.JPG", "actor/ignored.mp4", "actor/poster.jpg", "actor/film/main.mp4", "actor/film/photos/extra.mp4", "actor/film/photos/poster.jpg", "plain-film/main.mkv"] {
        touch(home.path(), file);
    }
    let report = inventory(home.path()).unwrap();
    assert_eq!(report.videos.iter().map(|v| v.relative_path.as_str()).collect::<Vec<_>>(), ["actor/film/main.mp4", "actor/film/photos/extra.mp4", "actor/ignored.mp4", "plain-film/main.mkv"]);
    assert_eq!(report.assets.iter().map(|a| a.relative_path.as_str()).collect::<Vec<_>>(), ["actor/actor.JPG", "actor/film/photos/poster.jpg", "actor/poster.jpg"]);
    assert_eq!(report.cover_checks.len(),2);
}

#[test]
fn saved_actor_identity_does_not_change_film_roles() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    touch(&root, "actor/film/movie.mp4");
    touch(home.path(), "actor.jpg"); // outside source is not actor evidence
    let storage = Storage::open(&home.path().join("data")).unwrap();
    storage.with_connection(|db| {
        db.execute("INSERT INTO actors(id,observed_name) VALUES('saved','actor')", [])?;
        db.execute("INSERT INTO actor_directories(actor_id,directory_key,directory_name) VALUES('saved',?1,'actor')", [directory_roles::key(&root.join("actor"))])?;
        Ok(())
    }).unwrap();
    let report = inventory(&root).unwrap();
    assert!(report.roles.iter().all(|r| r.role != Role::Actor));
    assert_eq!(report.videos.len(), 1);
    assert!(report.assets.is_empty());
    assert_eq!(inventory(&root).unwrap().videos.len(),1);
}

#[test]
fn explicitly_ignored_junction_neither_reads_outside_source_nor_makes_scan_incomplete() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("source");
    touch(&root, "film/main.mp4");
    let target = home.path().join("outside");
    touch(&target, "secret.mp4");
    let link = root.join("film/photos");
    #[cfg(windows)] {
        let status = std::process::Command::new("cmd").args(["/C", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\")).arg(target.to_string_lossy().replace('/', "\\")).output().unwrap();
        assert!(status.status.success(), "{status:?}");
    }
    #[cfg(unix)] std::os::unix::fs::symlink(&target, &link).unwrap();
    for broken in [false, true] {
        if broken { fs::remove_dir_all(&target).unwrap(); }
        let report = local_film_library_preview_lib::inventory::inventory_with_rules(&root,&Default::default(),&["photos".into()].into_iter().collect()).unwrap();
        assert_eq!(report.videos.len(), 1);
        assert!(report.issues.is_empty());
        assert_eq!(report.directories, ["", "film"]);
    }
}

#[test]
fn video_sizes_are_scan_evidence_including_nested_files() {
    let home = tempfile::tempdir().unwrap();
    touch(home.path(),"film/main.mp4"); touch(home.path(),"film/photos/ignored.mp4");
    fs::write(home.path().join("film/main.mp4"),b"1234567").unwrap();
    let before = inventory(home.path()).unwrap();
    assert_eq!(before.video_sizes.get("film/main.mp4"),Some(&7));
    assert_eq!(before.video_sizes.len(),2);
    fs::write(home.path().join("film/main.mp4"),b"12345678").unwrap();
    assert_ne!(before,inventory(home.path()).unwrap());
}

#[cfg(windows)]
#[test]
fn windows_actor_keys_match_extended_unc_paths_without_reading_them() {
    assert!(directory_roles::equivalent("//?/unc/SyntheticServer/share/Actor", "//syntheticserver/SHARE/actor"));
    assert!(directory_roles::equivalent("//?/C:/Synthetic/Actor", "c:/synthetic/actor"));
}
