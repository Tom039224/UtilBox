//! Small headless harness used to verify the conversion/compression engines
//! without launching the GUI. Not part of the shipped UI.
//!
//! Examples:
//!   cargo run --quiet --bin utilbox-cli -- formats
//!   cargo run --quiet --bin utilbox-cli -- convert png /tmp/out input.jpg
//!   cargo run --quiet --bin utilbox-cli -- compress in.mp4 out.mp4 --crf 26 --height 720
//!   cargo run --quiet --bin utilbox-cli -- probe in.mp4
//!   cargo run --quiet --bin utilbox-cli -- ytinfo "https://youtu.be/..."

use std::process::ExitCode;
use utilbox_lib::job::Job;
use utilbox_lib::tools::Tools;
use utilbox_lib::types::{CompressRequest, ConvertRequest, DownloadCompression, DownloadRequest};
use utilbox_lib::{compress, convert, download};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("エラー: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let tools = Tools::headless();
    match args[0].as_str() {
        "tools" => {
            print_json(&tools.list())?;
        }
        "formats" => {
            print_json(&convert::list_formats())?;
        }
        "detect" => {
            let paths: Vec<String> = args[1..].to_vec();
            if paths.is_empty() {
                return Err("ファイルを指定してください".into());
            }
            let files: Vec<_> = paths.iter().map(|p| convert::detect_file(p)).collect();
            print_json(&files)?;
        }
        "probe" => {
            let path = args.get(1).ok_or("ファイルを指定してください")?;
            print_json(&compress::probe_media(&tools, path)?)?;
        }
        "convert" => {
            let target = args.get(1).ok_or("出力形式を指定してください")?;
            let outdir = args.get(2).ok_or("出力フォルダを指定してください")?;
            let inputs: Vec<String> = args[3..].to_vec();
            if inputs.is_empty() {
                return Err("入力ファイルを指定してください".into());
            }
            let job = Job::headless("convert");
            let req = ConvertRequest {
                inputs,
                target: target.clone(),
                output_dir: outdir.clone(),
                overwrite: true,
            };
            let res = convert::convert_batch(&job, &tools, &req)?;
            print_json(&res)?;
            if res.failed > 0 {
                return Err(format!("{} 件の変換に失敗しました", res.failed));
            }
        }
        "compress" => {
            let input = args.get(1).ok_or("入力ファイルを指定してください")?;
            let output = args.get(2).ok_or("出力ファイルを指定してください")?;
            let mut req = CompressRequest {
                input: input.clone(),
                output: output.clone(),
                mode: "crf".into(),
                crf: Some(24),
                target_mb: None,
                max_height: None,
                codec: "h264".into(),
                audio_kbps: Some(128),
                remove_audio: false,
                preset: Some("medium".into()),
                audio_mode: Some("encode".into()),
            };
            let mut i = 3;
            while i < args.len() {
                let key = args[i].as_str();
                let val = args.get(i + 1).map(String::as_str);
                match key {
                    "--mode" => req.mode = val.unwrap_or("crf").to_string(),
                    "--crf" => {
                        req.crf = Some(
                            val.ok_or("--crf の値がありません")?
                                .parse()
                                .map_err(|_| "CRF が不正です")?,
                        )
                    }
                    "--target-mb" => {
                        req.target_mb = Some(
                            val.ok_or("--target-mb の値がありません")?
                                .parse()
                                .map_err(|_| "目標サイズが不正です")?,
                        )
                    }
                    "--height" => {
                        req.max_height = Some(
                            val.ok_or("--height の値がありません")?
                                .parse()
                                .map_err(|_| "解像度が不正です")?,
                        )
                    }
                    "--codec" => req.codec = val.unwrap_or("h264").to_string(),
                    "--audio-bits" => {
                        req.audio_kbps = Some(
                            val.ok_or("--audio-bits の値がありません")?
                                .parse()
                                .map_err(|_| "音声ビットレートが不正です")?,
                        )
                    }
                    "--no-audio" => {
                        req.remove_audio = true;
                        i += 1;
                        continue;
                    }
                    "--audio-copy" => req.audio_mode = Some("copy".into()),
                    "--preset" => req.preset = Some(val.unwrap_or("medium").to_string()),
                    other => return Err(format!("不明なオプション: {other}")),
                }
                i += 2;
            }
            let job = Job::headless("compress");
            print_json(&compress::compress(&job, &tools, &req)?)?;
        }
        "ytinfo" => {
            let url = args.get(1).ok_or("URL を指定してください")?;
            print_json(&download::probe_url(&tools, url)?)?;
        }
        "download" => {
            let url = args.get(1).ok_or("URL を指定してください")?;
            let outdir = args.get(2).ok_or("出力フォルダを指定してください")?;
            let mut req = DownloadRequest {
                url: url.clone(),
                output_dir: outdir.clone(),
                mode: "video".into(),
                quality: "1080".into(),
                audio_format: Some("mp3".into()),
                embed_thumbnail: false,
                embed_metadata: false,
                cookies_browser: None,
                compression: None,
            };
            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--audio" => {
                        req.mode = "audio".into();
                        i += 1;
                        continue;
                    }
                    "--quality" => req.quality = args.get(i + 1).cloned().unwrap_or("1080".into()),
                    "--format" => {
                        req.audio_format = args.get(i + 1).cloned().or(Some("mp3".into()))
                    }
                    "--compress" => {
                        ensure_compression(&mut req);
                        i += 1;
                        continue;
                    }
                    "--crf" => {
                        ensure_compression(&mut req).crf = Some(
                            args.get(i + 1)
                                .ok_or("--crf の値がありません")?
                                .parse()
                                .map_err(|_| "CRF が不正です")?,
                        )
                    }
                    "--target-mb" => {
                        let config = ensure_compression(&mut req);
                        config.mode = "size".into();
                        config.target_mb = Some(
                            args.get(i + 1)
                                .ok_or("--target-mb の値がありません")?
                                .parse()
                                .map_err(|_| "目標サイズが不正です")?,
                        );
                    }
                    "--codec" => {
                        ensure_compression(&mut req).codec =
                            args.get(i + 1).cloned().unwrap_or("h264".into())
                    }
                    "--height" => {
                        ensure_compression(&mut req).max_height = Some(
                            args.get(i + 1)
                                .ok_or("--height の値がありません")?
                                .parse()
                                .map_err(|_| "解像度が不正です")?,
                        )
                    }
                    other => return Err(format!("不明なオプション: {other}")),
                }
                i += 2;
            }
            let job = Job::headless("download");
            print_json(&download::download(&job, &tools, &req)?)?;
        }
        other => {
            return Err(format!("不明なコマンド: {other}"));
        }
    }
    Ok(())
}

fn ensure_compression(req: &mut DownloadRequest) -> &mut DownloadCompression {
    req.compression.get_or_insert_with(|| DownloadCompression {
        mode: "crf".into(),
        crf: Some(24),
        target_mb: None,
        max_height: None,
        codec: "h264".into(),
        audio_kbps: Some(128),
        remove_audio: false,
        preset: Some("medium".into()),
        audio_mode: Some("encode".into()),
    })
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

fn usage() {
    eprintln!(
        "UtilBox CLI\n\
         \n\
         utilbox-cli tools\n\
         utilbox-cli formats\n\
         utilbox-cli detect <files...>\n\
         utilbox-cli probe <file>\n\
         utilbox-cli convert <target-format> <output-dir> <files...>\n\
         utilbox-cli compress <input> <output> [--crf N] [--height H] [--codec h264|h265|av1|av1_gpu]\n\
         \x20                    [--mode crf|size] [--target-mb N] [--audio-bits N|--no-audio|--audio-copy]\n\
         utilbox-cli ytinfo <url>\n\
         utilbox-cli download <url> <output-dir> [--audio] [--quality 1080] [--format mp3]\n\
         \x20                    [--compress] [--crf N] [--codec h264|h265|av1|av1_gpu] [--height H] [--target-mb N]"
    );
}
