//! The binary is the part a person reads. These checks keep `--help` honest
//! and make sure a file round-trips through JSON.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use image::{Rgb, RgbImage};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_oceanmaster"))
}

#[test]
fn help_names_the_scores_and_the_knobs() {
    let output = bin().arg("--help").output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{text}");
    for needle in [
        "UICM",
        "UISM",
        "UIConM",
        "UIQM",
        "PSNR",
        "file name",
        "--uicm-alpha-l",
        "--uicm-alpha-r",
        "--uicm-mean-weight",
        "--uicm-var-weight",
        "--c1",
        "--c2",
        "--c3",
        "--before",
        "--block-size",
        "0.0282",
        "0.1",
        "-0.0268",
    ] {
        assert!(text.contains(needle), "help is missing {needle}:\n{text}");
    }
}

#[test]
fn json_report_for_one_png() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oceanmaster-cli-{stamp}"));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("reef.png");
    let mut image = RgbImage::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let v = if x < 4 { 30 } else { 80 + (y as u8) * 8 };
            image.put_pixel(x, y, Rgb([v, 40, 90]));
        }
    }
    image.save(&path).unwrap();

    let output = bin()
        .args([
            "-i",
            path.to_str().unwrap(),
            "--format",
            "json",
            "--metrics",
            "underwater",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    let uicm = parsed[0]["metrics"]["underwater"]["uicm"]["uicm"]
        .as_f64()
        .expect(&text);
    let uiqm = parsed[0]["metrics"]["underwater"]["uiqm"]
        .as_f64()
        .expect(&text);
    assert!(uicm.is_finite() && uiqm.is_finite(), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn folder_match_writes_infinite_psnr_to_csv() {
    let dir = temp_dir("match");
    let input = dir.join("enhanced");
    let reference = dir.join("original");
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&reference).unwrap();
    let pixels = RgbImage::from_pixel(8, 8, Rgb([12, 40, 90]));
    pixels.save(input.join("frame.png")).unwrap();
    pixels.save(reference.join("frame.jpg")).unwrap();
    let report = dir.join("report.csv");

    let output = bin()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-r",
            reference.to_str().unwrap(),
            "-o",
            report.to_str().unwrap(),
            "--format",
            "csv",
            "--metrics",
            "psnr",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let csv = fs::read_to_string(&report).unwrap();
    assert!(csv.lines().next().unwrap().contains("psnr"), "{csv}");
    assert!(csv.contains("frame.jpg"), "{csv}");
    assert!(csv.contains("inf"), "{csv}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unmatched_folder_file_warns_and_still_scores() {
    let dir = temp_dir("missing-ref");
    let input = dir.join("enhanced");
    let reference = dir.join("original");
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&reference).unwrap();
    RgbImage::from_pixel(8, 8, Rgb([20, 80, 90]))
        .save(input.join("only-here.png"))
        .unwrap();
    RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))
        .save(reference.join("somewhere-else.png"))
        .unwrap();

    let output = bin()
        .args([
            "-i",
            input.to_str().unwrap(),
            "-r",
            reference.to_str().unwrap(),
            "--format",
            "json",
            "--metrics",
            "uicm",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("no reference"),
        "expected a missing-match warning, got {stderr}"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        parsed[0]["metrics"]["underwater"]["uicm"]["uicm"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
    assert!(
        parsed[0]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("no reference"))
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn before_and_after_reports_the_change() {
    let dir = temp_dir("before-after");
    let before = dir.join("original.png");
    let after = dir.join("enhanced.png");
    RgbImage::from_pixel(16, 16, Rgb([40, 90, 80]))
        .save(&before)
        .unwrap();
    let mut processed = RgbImage::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let value = if x < 4 { 20 } else { 80 + (y as u8) * 8 };
            processed.put_pixel(x, y, Rgb([value, 30, 140]));
        }
    }
    processed.save(&after).unwrap();

    let output = bin()
        .args([
            "-i",
            after.to_str().unwrap(),
            "--before",
            before.to_str().unwrap(),
            "--format",
            "json",
            "--metrics",
            "uiqm",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let change = &parsed[0]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|change| change["name"] == "uiqm")
        .expect("uiqm change");
    let before_score = change["before"].as_f64().unwrap();
    let after_score = change["after"].as_f64().unwrap();
    let delta = change["delta"].as_f64().unwrap();
    assert!((delta - (after_score - before_score)).abs() < 1e-9);
    assert_ne!(before_score, after_score);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_metric_is_rejected() {
    let dir = temp_dir("bad-metric");
    let path = dir.join("reef.png");
    RgbImage::from_pixel(4, 4, Rgb([1, 2, 3]))
        .save(&path)
        .unwrap();
    let output = bin()
        .args(["-i", path.to_str().unwrap(), "--metrics", "not-a-score"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.to_ascii_lowercase().contains("unknown metric"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oceanmaster-{label}-{stamp}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}
