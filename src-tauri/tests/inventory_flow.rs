use local_film_library_preview_lib::inventory::{AssetKind, Prompt, inventory};
use std::fs::{self, File};
use std::path::Path;

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    File::create(path).unwrap();
}

#[test]
fn retains_source_direct_and_nested_suffix_videos() {
    let dir = tempfile::tempdir().unwrap();
    for path in ["top.mov", "无方括号/无编号.mkv", "无方括号/ABP-123(片段).mp4", "无方括号/ABP-123【中文字幕】.mp4", "无方括号/深层/ignored.mp4"] { touch(dir.path(), path); }
    let report = inventory(dir.path()).unwrap();
    let paths: Vec<_> = report.videos.iter().map(|v| v.relative_path.as_str()).collect();
    assert_eq!(paths,["top.mov","无方括号/ABP-123(片段).mp4","无方括号/ABP-123【中文字幕】.mp4","无方括号/无编号.mkv","无方括号/深层/ignored.mp4"]);
    assert_eq!(report.containers.len(),3);
    assert_eq!(report.videos[1].extension,"mp4");
    assert_eq!(report.videos[1].clues.file_stem,"ABP-123(片段)");
    assert_eq!(report.videos[1].clues.directory_names,["无方括号"]);
    assert!(report.containers.iter().any(|c|c.directory=="无方括号" && c.prompts.contains(&Prompt::MultipleVideos) && c.prompts.contains(&Prompt::NoBracketedCode)));
    assert!(report.containers.iter().any(|c|c.directory.is_empty()));
    assert!(report.videos.iter().find(|v|v.clues.file_stem=="无编号").unwrap().clues.code_candidates.is_empty());
}

#[test]
fn keeps_same_code_separate_and_reports_conflict_and_adjacent_assets() {
    let dir = tempfile::tempdir().unwrap();
    for path in [
        "演员甲/[FC2-PPV-1029384] 夜间笔记/FC2-PPV-1029384_original.mp4",
        "演员甲/[FC2-PPV-1029384] 夜间笔记/FC2-PPV-1029385_sample.mp4",
        "演员甲/[FC2-PPV-1029384] 夜间笔记/poster.jpg",
        "演员甲/[FC2-PPV-1029384] 夜间笔记/poster.webp",
        "演员甲/[FC2-PPV-1029384] 夜间笔记/one.srt",
        "演员甲/[FC2-PPV-1029384] 夜间笔记/two.ass",
        "演员甲/[FC2-PPV-1029384] 夜间笔记/gallery/still.png",
        "演员乙/[FC2-PPV-1029384] 别处/FC2-PPV-1029384【中文字幕】.mp4",
    ] {
        touch(dir.path(), path);
    }
    touch(dir.path(), "演员甲/actor.jpg"); touch(dir.path(), "演员乙/actor.jpg");
    let report = inventory(dir.path()).unwrap();
    assert_eq!(report.videos.len(), 3);
    assert_eq!(report.containers.len(), 2);
    assert_eq!(
        report
            .assets
            .iter()
            .filter(|a| a.kind == AssetKind::Image)
            .count(),
        5
    );
    assert_eq!(
        report
            .assets
            .iter()
            .filter(|a| a.kind == AssetKind::Subtitle)
            .count(),
        2
    );
    let first = report
        .containers
        .iter()
        .find(|c| c.directory.contains("夜间笔记"))
        .unwrap();
    assert_eq!(first.video_paths.len(), 2);
    assert!(!first.asset_paths.iter().any(|p| p.ends_with("gallery/still.png")));
    for prompt in [
        Prompt::MultipleSubtitles,
        Prompt::RepeatedCodeAcrossDirectories,
    ] {
        assert!(first.prompts.contains(&prompt), "missing {prompt:?}");
    }
    assert_eq!(
        report
            .videos
            .iter()
            .find(|v| v.relative_path.contains("1029385_sample"))
            .unwrap()
            .clues
            .code_candidates,
        ["FC2-PPV-1029384"]
    );
}

#[test]
fn reports_links_without_traversing_outside_root() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    touch(outside.path(), "outside.mp4");
    touch(dir.path(), "film/inside.mp4");
    let link = dir.path().join("linked");
    #[cfg(windows)]
    let created = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .output()
        .unwrap();
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(outside.path(), &link);
    #[cfg(windows)]
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    #[cfg(unix)]
    created.unwrap();
    let report = inventory(dir.path()).unwrap();
    assert_eq!(report.videos.len(), 1);
    assert!(report.issues.iter().any(|i| i.relative_path == "linked"));
}

#[test]
fn retains_supported_video_extensions_through_nested_directories() {
    let dir = tempfile::tempdir().unwrap();
    for path in [
        "分类甲/虚构甲.mpg",
        "分类甲/虚构乙.rm",
        "分类甲/虚构丙.RMVB",
        "分类甲/演员占位/[XYZ-101] 虚构片/XYZ-101.mp4",
        "分类甲/图片专用/portrait.jpg",
    ] {
        touch(dir.path(), path);
    }

    let report = inventory(dir.path()).unwrap();
    assert_eq!(report.videos.len(), 4);
    assert_eq!(
        report
            .videos
            .iter()
            .filter(|v| v.directory == "分类甲")
            .count(),
        3
    );
    assert!(
        report
            .videos
            .iter()
            .any(|v| v.relative_path == "分类甲/虚构甲.mpg" && v.extension == "mpg")
    );
    assert!(
        report
            .videos
            .iter()
            .any(|v| v.relative_path == "分类甲/虚构乙.rm" && v.extension == "rm")
    );
    assert!(
        report
            .videos
            .iter()
            .any(|v| v.relative_path == "分类甲/虚构丙.RMVB" && v.extension == "rmvb")
    );
    let direct = report
        .containers
        .iter()
        .find(|c| c.directory == "分类甲")
        .unwrap();
    assert_eq!(direct.video_paths.len(), 3);
    assert!(direct.prompts.contains(&Prompt::MultipleVideos));
    assert!(
        !report
            .containers
            .iter()
            .any(|c| c.directory == "分类甲/图片专用")
    );
    assert!(report.assets.iter().any(|a|a.relative_path=="分类甲/图片专用/portrait.jpg"));
}

#[test]
fn number_evidence_is_only_the_first_nonempty_ascii_bracket_in_the_film_folder() {
 let temp=tempfile::tempdir().unwrap();
 for directory in ["[中文自定编号] title","[CUSTOM] [SECOND] title","no code","【FULL-123】 title"] {
  std::fs::create_dir_all(temp.path().join(directory)).unwrap();std::fs::write(temp.path().join(directory).join("FILE-999.mp4"),[]).unwrap();
 }
 let report=local_film_library_preview_lib::inventory::inventory(temp.path()).unwrap();
 for video in report.videos {
  let expected=match video.directory.as_str(){"[中文自定编号] title"=>vec!["中文自定编号"],"[CUSTOM] [SECOND] title"=>vec!["CUSTOM"],_=>vec![]};assert_eq!(video.clues.code_candidates,expected);
 }
}
