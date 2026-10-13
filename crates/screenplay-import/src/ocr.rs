//! OCR Sidecar：通过调用 Swift 编译的二进制（基于 Apple Vision Framework）
//! 对图片或扫描 PDF 进行文本识别，再走剧本解析器生成 AST。
//!
//! 调用约定：见 `swift/ocr_sidecar.swift` 顶部注释。
//! 输出格式：
//! ```text
//! PAGE 1
//! 0.98\tEXT. BAR - NIGHT
//! 0.95\tJOHN
//! ...
//! END
//! ```

use screenplay_core::parser::{self, ParseOptions, ParseOutcome};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{Read, BufRead};

/// 单页 OCR 结果（已按阅读顺序排序）
#[derive(Debug, Clone)]
pub struct OcrPage {
    pub page_no: u32,
    pub lines: Vec<OcrLine>,
}

#[derive(Debug, Clone)]
pub struct OcrLine {
    pub confidence: f64,
    pub text: String,
}

/// 解析 sidecar 输出
pub fn parse_sidecar_output(stdout: &str) -> Vec<OcrPage> {
    let mut pages = Vec::new();
    let mut cur_page: Option<OcrPage> = None;
    for line in stdout.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("PAGE ") {
            if let Some(p) = cur_page.take() {
                pages.push(p);
            }
            let n: u32 = rest.parse().unwrap_or(0);
            cur_page = Some(OcrPage { page_no: n, lines: Vec::new() });
        } else if line == "END" {
            if let Some(p) = cur_page.take() {
                pages.push(p);
            }
            break;
        } else {
            // 形如 "0.95\t文本"
            let mut it = line.splitn(2, '\t');
            let conf_str = it.next().unwrap_or("0");
            let text = it.next().unwrap_or("");
            let confidence: f64 = conf_str.parse().unwrap_or(0.0);
            if let Some(p) = cur_page.as_mut() {
                p.lines.push(OcrLine { confidence, text: text.to_string() });
            }
        }
    }
    if let Some(p) = cur_page {
        pages.push(p);
    }
    pages
}

/// 找到 sidecar 可执行文件路径：
/// 1. `SCREENPLAY_OCR_SIDECAR` 环境变量
/// 2. 与当前可执行文件同目录（Tauri app 打包到 Resources/）
/// 3. Cargo manifest 目录下的 swift/ocr_sidecar（开发与测试）
pub fn find_sidecar() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SCREENPLAY_OCR_SIDECAR") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    // Tauri app：打包时 sidecar 复制到 Resources/，运行时 .app/Contents/Resources/{path}
    if let Ok(exe) = std::env::current_exe() {
        // exe 通常为 .app/Contents/MacOS/<binary>，向上两级为 Contents，再/Resources
        if let Some(macos_dir) = exe.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                let candidates = [
                    // Tauri 把 resources/ocr_sidecar 映射到 Resources/resources/ocr_sidecar
                    contents_dir.join("Resources").join("resources").join("ocr_sidecar"),
                    contents_dir.join("Resources").join("ocr_sidecar"),
                    contents_dir.join("Resources").join("ocr_sidecar.app").join("Contents/MacOS/ocr_sidecar"),
                    macos_dir.join("ocr_sidecar"),
                ];
                for c in candidates {
                    if c.exists() {
                        return Some(c);
                    }
                }
            }
        }
        // 单元测试场景：target/debug/ 下运行，找同目录的 ocr_sidecar
        let sibling = exe.with_file_name("ocr_sidecar");
        if sibling.exists() {
            return Some(sibling);
        }
    }
    // 开发场景：从 manifest dir 找
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest).join("swift").join("ocr_sidecar");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// 对一个文件（图片或 PDF）执行 OCR，返回分页结果
pub fn run_ocr(path: &Path, langs: Option<&str>) -> Result<Vec<OcrPage>, OcrError> {
    run_ocr_controlled(path, langs, &crate::ImportControl::default())
}

pub fn run_ocr_controlled(path: &Path, langs: Option<&str>, control: &crate::ImportControl) -> Result<Vec<OcrPage>, OcrError> { run_controlled(path,langs,control,false) }
pub fn run_layout_controlled(path: &Path, langs: Option<&str>, control: &crate::ImportControl) -> Result<Vec<OcrPage>, OcrError> { run_controlled(path,langs,control,true) }
fn run_controlled(path: &Path, langs: Option<&str>, control: &crate::ImportControl, layout:bool) -> Result<Vec<OcrPage>, OcrError> {
    if control.is_cancelled() { return Err(OcrError::Cancelled); }
    control.report("ocr", 0, 0);
    let sidecar = find_sidecar().ok_or(OcrError::SidecarNotFound)?;
    let mut cmd = Command::new(sidecar);
    cmd.arg(path).arg("--text-first");
    if layout {cmd.arg("--layout");}
    if let Some(l) = langs {
        cmd.arg("--langs").arg(l);
    }
    let output = run_process(cmd, control)?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(OcrError::ExitNonZero(output.status.code().unwrap_or(-1), err));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(parse_sidecar_output(&stdout))
}

fn run_process(mut cmd: Command, control: &crate::ImportControl) -> Result<std::process::Output, OcrError> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|e| OcrError::SpawnFailed(e.to_string()))?;
    let mut stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    // Drain both pipes concurrently so large OCR results cannot deadlock the child.
    let out = std::thread::spawn(move || { let mut bytes = Vec::new(); stdout.read_to_end(&mut bytes).map(|_| bytes) });
    let progress=control.clone();
    let err = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut bytes=Vec::new();
        for line in std::io::BufReader::new(stderr).lines() {
            let line=line?;
            if let Some(rest)=line.strip_prefix("PROGRESS ") {let numbers=rest.split_whitespace().filter_map(|n|n.parse::<usize>().ok()).collect::<Vec<_>>();if numbers.len()==2 {progress.report("pdf_pages",numbers[0],numbers[1]);}}
            else {bytes.extend_from_slice(line.as_bytes());bytes.push(b'\n');}
        }
        Ok(bytes)
    });
    let status = loop {
        if control.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out.join();
            let _ = err.join();
            return Err(OcrError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(30)),
            Err(e) => { let _ = child.kill(); let _ = child.wait(); return Err(OcrError::SpawnFailed(e.to_string())); }
        }
    };
    let stdout = out.join().map_err(|_| OcrError::SpawnFailed("OCR 输出读取失败".into()))?
        .map_err(|e| OcrError::SpawnFailed(e.to_string()))?;
    let stderr = err.join().map_err(|_| OcrError::SpawnFailed("OCR 错误读取失败".into()))?
        .map_err(|e| OcrError::SpawnFailed(e.to_string()))?;
    Ok(std::process::Output { status, stdout, stderr })
 }

/// OCR 错误
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("导入已取消")]
    Cancelled,
    #[error("找不到 OCR sidecar 二进制（请设置 SCREENPLAY_OCR_SIDECAR 或打包到 Resources/）")]
    SidecarNotFound,
    #[error("启动 sidecar 失败: {0}")]
    SpawnFailed(String),
    #[error("sidecar 退出码 {0}: {1}")]
    ExitNonZero(i32, String),
}

/// 主入口：对图片或扫描 PDF 执行 OCR，生成剧本 AST
pub fn import_ocr(path: &Path, title: Option<String>, langs: Option<&str>) -> Result<ParseOutcome, OcrError> {
    import_ocr_controlled(path, title, langs, &crate::ImportControl::default())
}

pub fn import_ocr_controlled(path: &Path, title: Option<String>, langs: Option<&str>, control: &crate::ImportControl) -> Result<ParseOutcome, OcrError> {
    let pages = run_ocr_controlled(path, langs, control)?;
    if control.is_cancelled() { return Err(OcrError::Cancelled); }
    let mut text = String::new();
    for page in &pages {
        for line in &page.lines {
            text.push_str(&line.text);
            text.push('\n');
        }
        text.push('\n');
    }
    let opts = ParseOptions {
        title,
        denoise: true,
        known_characters: Vec::new(),
        layout_hints: Vec::new(),
    };
    Ok(parser::parse(&text, &opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn cancellation_terminates_running_sidecar() {
        let control = crate::ImportControl::default();
        let worker_control = control.clone();
        let started = std::time::Instant::now();
        let worker = std::thread::spawn(move || {
            let mut command = Command::new("/bin/sleep");
            command.arg("10");
            run_process(command, &worker_control)
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        control.cancel();
        assert!(matches!(worker.join().unwrap(), Err(OcrError::Cancelled)));
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "取消应终止子进程而不是等待完成");
    }

    #[test]
    fn t01_parse_sidecar_output_basic() {
        let stdout = "PAGE 1\n0.98\tEXT. BAR - NIGHT\n0.95\tJOHN\n0.97\tHello, world.\nEND\n";
        let pages = parse_sidecar_output(stdout);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page_no, 1);
        assert_eq!(pages[0].lines.len(), 3);
        assert!((pages[0].lines[0].confidence - 0.98).abs() < 1e-6);
        assert_eq!(pages[0].lines[0].text, "EXT. BAR - NIGHT");
    }

    #[test]
    fn t02_parse_sidecar_multi_pages() {
        let stdout = "PAGE 1\n0.95\tline A\nPAGE 2\n0.90\tline B\n0.88\tline C\nEND\n";
        let pages = parse_sidecar_output(stdout);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].lines.len(), 1);
        assert_eq!(pages[1].lines.len(), 2);
        assert_eq!(pages[1].lines[1].text, "line C");
    }

    #[test]
    fn t03_parse_empty_output_returns_empty() {
        let pages = parse_sidecar_output("");
        assert!(pages.is_empty());
    }

    #[test]
    fn t04_parse_malformed_lines_skipped_gracefully() {
        // 缺少置信度的行：conf 解析为 0，但文本仍保留
        let stdout = "PAGE 1\nno_number_here\tstill_text\n0.5\tgood\nEND\n";
        let pages = parse_sidecar_output(stdout);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].lines.len(), 2);
        assert_eq!(pages[0].lines[0].confidence, 0.0);
        assert_eq!(pages[0].lines[0].text, "still_text");
    }

    /// 端到端：调用真实的 sidecar 二进制对样本图片进行 OCR
    /// 跳过条件：sidecar 不存在（非 macOS 或未编译）
    #[test]
    fn t05_end_to_end_ocr_sample_image() {
        let sidecar = match find_sidecar() {
            Some(p) => p,
            None => {
                eprintln!("跳过：未找到 OCR sidecar");
                return;
            }
        };
        // 找样本图片
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let sample = PathBuf::from(manifest).join("swift").join("ocr_sample.png");
        if !sample.exists() {
            eprintln!("跳过：未找到样本图片 {sample:?}");
            return;
        }
        let pages = run_ocr(&sample, Some("en-US")).expect("OCR 应成功");
        assert!(!pages.is_empty(), "应至少有一页");
        let joined: String = pages
            .iter()
            .flat_map(|p| p.lines.iter().map(|l| l.text.clone()))
            .collect::<Vec<_>>()
            .join("|");
        assert!(joined.contains("CITY STREET"), "OCR 结果应包含场景标题: {joined}");
        assert!(joined.contains("JOHN") || joined.contains("MARIA"), "OCR 结果应包含人物名: {joined}");
        let _ = sidecar; // 抑制未使用警告
    }
}
