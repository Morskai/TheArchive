use local_film_library_preview_lib::inventory::inventory;
use local_film_library_preview_lib::proposal::{Disposition, propose};
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Instant;

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    if run_session(stdin.lock(), &mut stdout).is_err() {
        let _ = writeln!(stdout, "诊断未完成：输入或输出不可用。未保存报告。");
        std::process::exit(1);
    }
}

fn run_session<R: BufRead, W: Write>(mut input: R, output: &mut W) -> io::Result<()> {
    writeln!(
        output,
        "只读扫描诊断：请输入片库根目录的完整路径；直接回车取消。"
    )?;
    output.flush()?;
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        writeln!(output, "已取消。")?;
        return Ok(());
    }
    let path = line.trim().trim_matches('"');
    if path.is_empty() {
        writeln!(output, "已取消。")?;
        return Ok(());
    }
    let started = Instant::now();
    let report = match inventory(Path::new(path)) {
        Ok(report) => report,
        Err(_) => {
            writeln!(output, "诊断未完成：根目录无效或不可访问。未保存报告。")?;
            return Ok(());
        }
    };
    let proposals = propose(&report);
    let mut extensions = BTreeMap::<&str, usize>::new();
    for video in &report.videos {
        *extensions.entry(video.extension.as_str()).or_default() += 1;
    }
    let candidates = proposals
        .items
        .iter()
        .filter(|item| item.disposition == Disposition::SingleVideoCandidate)
        .count();
    let needs_review = proposals.items.len() - candidates;
    let mut issue_types = BTreeMap::<&str, usize>::new();
    for issue in proposals.issues {
        *issue_types.entry(issue.kind.as_str()).or_default() += 1;
    }
    writeln!(output, "视频总数：{}", report.videos.len())?;
    writeln!(output, "按扩展名计数：")?;
    for (extension, count) in extensions {
        writeln!(output, "  .{extension} {count}")?;
    }
    writeln!(output, "逐视频候选：{candidates}")?;
    writeln!(output, "待核对：{needs_review}")?;
    writeln!(output, "遍历问题：{}", proposals.issues.len())?;
    for (kind, count) in issue_types {
        writeln!(output, "  {kind} {count}")?;
    }
    writeln!(output, "耗时：{} 毫秒", started.elapsed().as_millis())?;
    if !proposals.issues.is_empty() {
        writeln!(
            output,
            "结果不完整：存在遍历问题，以上数量仅代表成功枚举的条目。"
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run_session;
    use std::fs::{self, File};
    use std::io::Cursor;
    use std::path::Path;

    fn touch(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap();
    }

    #[test]
    fn empty_input_cancels_without_scanning() {
        let mut output = Vec::new();
        run_session(Cursor::new("\n"), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("已取消"));
        assert!(!text.contains("视频总数"));
    }

    #[test]
    fn reports_video_and_proposal_counts_without_paths() {
        let root = tempfile::tempdir().unwrap();
        touch(root.path(), "演员/[ABC-001] 片名/ABC-001.mp4");
        touch(root.path(), "其它影片/直放.wmv");
        touch(root.path(), "其它影片/另一个.mov");
        touch(root.path(), "演员/actor.jpg");
        let mut output = Vec::new();
        let input = format!("{}\n", root.path().display());
        run_session(Cursor::new(input), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("视频总数：3"));
        assert!(text.contains(".mp4 1"));
        assert!(text.contains(".wmv 1"));
        assert!(text.contains(".mov 1"));
        assert!(text.contains("逐视频候选：1"));
        assert!(text.contains("待核对：2"));
        assert!(text.contains("遍历问题：0"));
        assert!(text.contains("耗时："));
        assert!(!text.contains(&root.path().display().to_string()));
        assert!(!text.contains("ABC-001"));
        assert!(!text.contains("直放"));
    }

    #[test]
    fn traversal_issue_marks_result_incomplete_without_disclosing_name() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        touch(root.path(), "film/inside.mp4");
        touch(outside.path(), "outside.mp4");
        let link = root.path().join("secret-link");
        #[cfg(windows)]
        {
            let created = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(outside.path())
                .output()
                .unwrap();
            assert!(created.status.success());
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        let mut output = Vec::new();
        run_session(
            Cursor::new(format!("{}\n", root.path().display())),
            &mut output,
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("视频总数：1"));
        assert!(text.contains("遍历问题：1"));
        assert!(text.contains("link_skipped 1"));
        assert!(text.contains("结果不完整"));
        assert!(!text.contains("secret-link"));
        assert!(!text.contains(&root.path().display().to_string()));
    }
}
