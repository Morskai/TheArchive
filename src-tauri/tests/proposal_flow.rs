use local_film_library_preview_lib::{
    inventory::{Prompt, inventory},
    proposal::{Disposition, ReviewReason, propose},
};
use std::{fs, path::Path};
fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}

#[test]
fn source_root_and_nested_video_directories_create_proposals() {
    let home = tempfile::tempdir().unwrap();
    touch(home.path(), "ignored-root.mp4");
    touch(home.path(), "unmarked/main.mp4");
    touch(home.path(), "unmarked/photos/ignored.mp4");
    let evidence = inventory(home.path()).unwrap();
    let result = propose(&evidence);
    assert_eq!(result.items.len(), 3);
    assert!(result.items.iter().any(|i|i.video.relative_path=="unmarked/main.mp4"));
    assert_eq!(
        result.items[0].disposition,
        Disposition::SingleVideoCandidate
    );
    assert!(result.items[0].reasons.is_empty());
}
#[test]
fn actor_direct_videos_and_descendant_videos_create_proposals() {
    let home = tempfile::tempdir().unwrap();
    touch(home.path(), "actor/actor.jpg");
    touch(home.path(), "actor/ignored.mp4");
    touch(home.path(), "actor/[QRS-101] film/QRS-101.mp4");
    let evidence = inventory(home.path()).unwrap();
    let result = propose(&evidence);
    assert_eq!(result.items.len(), 2);
    assert_eq!(
        result.items[0].disposition,
        Disposition::SingleVideoCandidate
    );
    assert!(
        propose(&inventory(&home.path().join("actor/[QRS-101] film")).unwrap())
            .items
            .len()==1
    );
}
#[test]
fn one_proposal_per_root_video_keeps_peers_and_direct_assets_only() {
    let home = tempfile::tempdir().unwrap();
    for path in [
        "[TEST-01]/part1.mp4",
        "[TEST-01]/part2.mp4",
        "[TEST-01]/poster.jpg",
        "[TEST-01]/other.jpg",
        "[TEST-01]/text.srt",
        "[TEST-01]/gallery/ignored.jpg",
    ] {
        touch(home.path(), path);
    }
    let evidence = inventory(home.path()).unwrap();
    let result = propose(&evidence);
    assert_eq!(result.items.len(), 2);
    for item in result.items {
        assert_eq!(item.peer_video_paths.len(), 1);
        assert_eq!(item.adjacent_asset_paths.len(), 3);
        assert_eq!(item.reasons, [ReviewReason::MultipleVideosInDirectory]);
    }
}
#[test]
fn same_code_in_different_film_directories_is_not_merged() {
    let home = tempfile::tempdir().unwrap();
    touch(home.path(), "[ABC-123] first/ABC-123.mp4");
    touch(home.path(), "[ABC-123] second/ABC-123.mp4");
    let evidence = inventory(home.path()).unwrap();
    let result = propose(&evidence);
    assert_eq!(result.items.len(), 2);
    assert_ne!(
        result.items[0].video.directory,
        result.items[1].video.directory
    );
    assert!(result.items.iter().all(|item| {
        item.container_prompts
            .contains(&Prompt::RepeatedCodeAcrossDirectories)
    }));
}
#[test]
fn folder_number_is_literal_and_filename_difference_is_not_a_conflict() {
    let home = tempfile::tempdir().unwrap();
    touch(home.path(), "[DEMO-106-C] film/DEMO-106.mp4");
    touch(home.path(), "[DEMO-107] film/DEMO-107.mp4");
    let evidence = inventory(home.path()).unwrap();
    let result = propose(&evidence);
    assert_eq!(result.items.len(), 2);
    let conflict = result
        .items
        .iter()
        .find(|i| i.video.relative_path.ends_with("DEMO-106.mp4"))
        .unwrap();
    assert!(conflict.reasons.is_empty());
    assert_eq!(conflict.video.clues.code_candidates, ["DEMO-106-C"]);
    let ordinary = result
        .items
        .iter()
        .find(|i| i.video.relative_path.ends_with("DEMO-107.mp4"))
        .unwrap();
    assert!(ordinary.reasons.is_empty());
}
#[test]
fn source_level_traversal_issues_survive_proposal_creation() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    touch(home.path(), "film/inside.mp4");
    let link = home.path().join("linked");
    #[cfg(windows)]
    {
        assert!(
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(outside.path())
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    let evidence = inventory(home.path()).unwrap();
    let result = propose(&evidence);
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.issues, evidence.issues);
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.kind == "link_skipped")
    );
    #[cfg(windows)]
    fs::remove_dir(link).unwrap();
}
