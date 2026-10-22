//! Command-line front end for the `oceanmaster` library.
//!
//! The formulas live in the library and are documented with `cargo doc`.
//! This file only parses arguments, matches files, and prints the report.

use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use comfy_table::{ContentArrangement, Table};
use indicatif::{ProgressBar, ProgressStyle};
use oceanmaster::{
    Comparison, Config, Error, MetricSet, Report, Result, UicmParams, UiqmParams, compare_files,
    discover, evaluate_job, parse_metrics,
};
use rayon::prelude::*;

const ABOUT: &str = "\
Measure how an image looks. Made for underwater photos, and it also computes
the usual error and contrast numbers.

Give --input a file or a folder. Add --before to compare quality scores
before and after processing. Add --reference when you want pixel error
(MSE, RMSE, MAE, PSNR). A partner folder is matched by file name.";

const AFTER_HELP: &str = "\
Underwater scores (no reference needed):
  UICM     colorfulness — is the picture stuck on green or blue?
  UISM     sharpness
  UIConM   contrast
  UIQM     one combined score: c1*UICM + c2*UISM + c3*UIConM

Other scores that need no reference:
  Michelson contrast, EME, weighted grayscale angle, histogram statistics

Scores that need a reference of the same size:
  MSE, RMSE, MAE, PSNR

--before compares the no-reference scores of the unprocessed image and
--input. The change column is after minus before. A positive change means
the score went up. For UIQM, up is the direction the paper treats as better.
If you also ask for MSE or PSNR and omit --reference, those are computed
against the --before image.

--metrics accepts a preset or a comma-separated list.
  auto         no-reference scores, plus MSE/RMSE/MAE/PSNR if --reference is set
  all          every score that can run
  underwater   UICM, UISM, UIConM, UIQM
  noref        Michelson, EME, grayscale angle, histogram, and underwater
  reference    MSE, RMSE, MAE, PSNR
  uicm,psnr    any mix of names, for example uiqm,michelson,rmse

Examples:
  oceanmaster -i reef.jpg
  oceanmaster -i ./photos -o report.csv
  oceanmaster -i enhanced.png -r original.png
  oceanmaster -i enhanced.png --before original.png
  oceanmaster -i ./enhanced --before ./original --metrics underwater
  oceanmaster -i ./enhanced -r ./original --metrics underwater,psnr
  oceanmaster -i reef.jpg --uicm-alpha-r 0.2 --c3 1.0";

#[derive(Parser)]
#[command(
    name = "oceanmaster",
    version,
    about = "Measure image quality, especially for underwater photos",
    long_about = ABOUT,
    after_help = AFTER_HELP,
    arg_required_else_help = true
)]
struct Cli {
    /// Image file or folder to score.
    ///
    /// Folders are not walked into subfolders unless you pass --recursive.
    /// Accepted types: PNG, JPEG, WebP, TIFF, BMP, GIF, TGA.
    #[arg(short, long, value_name = "PATH", help_heading = "Input")]
    input: PathBuf,

    /// Reference image or folder for MSE, RMSE, MAE, and PSNR.
    ///
    /// Two files are compared even if their names differ. Two folders are
    /// matched by the same relative path, then by the same file stem if the
    /// extension differs (frame.png can match frame.jpg).
    #[arg(short, long, value_name = "PATH", help_heading = "Input")]
    reference: Option<PathBuf>,

    /// Unprocessed image or folder. --input is the image after processing.
    ///
    /// Prints each no-reference score for both images, and the change
    /// (after - before). Folders match by file name, the same way as
    /// --reference. This does not replace --reference: pixel error still
    /// uses --reference when you pass one, and otherwise uses this image.
    #[arg(long, value_name = "PATH", help_heading = "Input")]
    before: Option<PathBuf>,

    /// Also score images in subfolders.
    #[arg(long, help_heading = "Input")]
    recursive: bool,

    /// Write the report to this file as well as to the terminal.
    ///
    /// The format follows --format. If you omit --format, a .json or .csv
    /// extension selects that format. Anything else stays a table on the
    /// terminal and is written as a table too.
    #[arg(short, long, value_name = "FILE", help_heading = "Output")]
    output: Option<PathBuf>,

    /// table, json, or csv.
    ///
    /// table is for reading. json and csv are for scripts. Histogram bin
    /// counts are included only in json.
    #[arg(
        short,
        long,
        value_enum,
        value_name = "FORMAT",
        help_heading = "Output"
    )]
    format: Option<Format>,

    /// Worker threads. Default: one per logical CPU.
    #[arg(short = 'j', long, value_name = "N", help_heading = "Output")]
    threads: Option<usize>,

    /// Which scores to compute. Default: auto.
    ///
    /// See the text below the flags for the preset names.
    #[arg(
        short,
        long,
        value_name = "LIST",
        default_value = "auto",
        help_heading = "What to measure"
    )]
    metrics: String,

    /// UIQM weight on colorfulness (UICM).
    #[arg(
        long,
        default_value = "0.0282",
        value_name = "WEIGHT",
        help_heading = "UIQM weights"
    )]
    c1: f64,

    /// UIQM weight on sharpness (UISM).
    #[arg(
        long,
        default_value = "0.2953",
        value_name = "WEIGHT",
        help_heading = "UIQM weights"
    )]
    c2: f64,

    /// UIQM weight on contrast (UIConM).
    #[arg(
        long,
        default_value = "3.5753",
        value_name = "WEIGHT",
        help_heading = "UIQM weights"
    )]
    c3: f64,

    /// UICM left-tail trim. 0.1 drops the lowest 10% of the RG and YB samples.
    #[arg(
        long,
        default_value = "0.1",
        value_name = "RATIO",
        help_heading = "UICM"
    )]
    uicm_alpha_l: f64,

    /// UICM right-tail trim. 0.1 drops the brightest 10%. Raise this for bubbles or rays.
    #[arg(
        long,
        default_value = "0.1",
        value_name = "RATIO",
        help_heading = "UICM"
    )]
    uicm_alpha_r: f64,

    /// UICM weight on the color-cast term, sqrt(mu_RG^2 + mu_YB^2).
    #[arg(
        long,
        default_value = "-0.0268",
        value_name = "WEIGHT",
        help_heading = "UICM"
    )]
    uicm_mean_weight: f64,

    /// UICM weight on the color-spread term, sqrt(sigma_RG^2 + sigma_YB^2).
    #[arg(
        long,
        default_value = "0.1586",
        value_name = "WEIGHT",
        help_heading = "UICM"
    )]
    uicm_var_weight: f64,

    /// Tile side, in pixels, for EME, block Michelson, UISM, and UIConM.
    ///
    /// The paper uses 8. Tiles that would hang off the edge are skipped.
    #[arg(
        long,
        default_value = "8",
        value_name = "PIXELS",
        help_heading = "Blocks"
    )]
    block_size: u32,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    /// Aligned columns on the terminal.
    Table,
    /// One JSON array of reports.
    Json,
    /// One row per image, with a header.
    Csv,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    if let Some(0) = cli.threads {
        return Err(Error::Invalid("threads must be at least 1".to_string()));
    }
    let metrics = parse_metrics(&cli.metrics, cli.reference.is_some())?;
    let mut config = Config::default().with_block_size(cli.block_size);
    config.metrics = metrics;
    config.uiqm = UiqmParams {
        c1: cli.c1,
        c2: cli.c2,
        c3: cli.c3,
    };
    config.uicm = UicmParams {
        alpha_l: cli.uicm_alpha_l,
        alpha_r: cli.uicm_alpha_r,
        mean_weight: cli.uicm_mean_weight,
        variance_weight: cli.uicm_var_weight,
    };
    config.validate()?;

    if let Some(threads) = cli.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .expect("thread pool is initialized once, at startup");
    }

    if cli.before.is_some() {
        return run_comparison(&cli, &config);
    }

    let jobs = discover(&cli.input, cli.reference.as_deref(), cli.recursive)?;
    let progress = if jobs.len() > 1 {
        let bar = ProgressBar::new(jobs.len() as u64);
        bar.set_style(
            ProgressStyle::with_template("{spinner} [{bar:40}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        Some(bar)
    } else {
        None
    };

    let mut outcomes: Vec<(usize, std::result::Result<Report, String>)> = jobs
        .par_iter()
        .enumerate()
        .map(|(index, job)| {
            if let Some(bar) = &progress {
                bar.set_message(job.input.display().to_string());
            }
            let outcome = evaluate_job(job, &config).map_err(|err| err.to_string());
            if let Some(bar) = &progress {
                bar.inc(1);
            }
            (index, outcome)
        })
        .collect();
    if let Some(bar) = &progress {
        bar.finish_and_clear();
    }
    outcomes.sort_by_key(|(index, _)| *index);

    let mut reports = Vec::new();
    let mut failed = 0_usize;
    for (_, outcome) in outcomes {
        match outcome {
            Ok(report) => {
                for warning in &report.warnings {
                    eprintln!("warning: {}: {warning}", report.path.display());
                }
                reports.push(report);
            }
            Err(err) => {
                failed += 1;
                eprintln!("error: {err}");
            }
        }
    }

    let format = resolve_format(cli.format, cli.output.as_deref());
    write_reports(&reports, &metrics, format, &mut io::stdout())?;
    if let Some(path) = &cli.output {
        let mut file = File::create(path).map_err(|source| Error::Write {
            path: path.clone(),
            source,
        })?;
        write_reports(&reports, &metrics, format, &mut file)?;
    }
    if failed > 0 {
        eprintln!("scored {} image(s), {failed} failed", reports.len());
    }
    Ok(failed == 0)
}

fn run_comparison(cli: &Cli, config: &Config) -> Result<bool> {
    let before = cli
        .before
        .as_deref()
        .expect("run_comparison is only called when --before is set");
    let jobs = discover(&cli.input, Some(before), cli.recursive)?;
    let pixel_reference = cli.reference.clone();
    let progress = if jobs.len() > 1 {
        let bar = ProgressBar::new(jobs.len() as u64);
        bar.set_style(
            ProgressStyle::with_template("{spinner} [{bar:40}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        Some(bar)
    } else {
        None
    };

    let mut outcomes: Vec<(usize, std::result::Result<Comparison, String>)> =
        jobs.par_iter()
            .enumerate()
            .map(|(index, job)| {
                if let Some(bar) = &progress {
                    bar.set_message(job.input.display().to_string());
                }
                let outcome = match &job.reference {
                    Some(before_path) => {
                        compare_files(&job.input, before_path, pixel_reference.as_deref(), config)
                            .map_err(|err| err.to_string())
                    }
                    None => Err(job.warning.clone().unwrap_or_else(|| {
                        format!("no before image matched {}", job.input.display())
                    })),
                };
                if let Some(bar) = &progress {
                    bar.inc(1);
                }
                (index, outcome)
            })
            .collect();
    if let Some(bar) = &progress {
        bar.finish_and_clear();
    }
    outcomes.sort_by_key(|(index, _)| *index);

    let mut comparisons = Vec::new();
    let mut failed = 0_usize;
    for (_, outcome) in outcomes {
        match outcome {
            Ok(comparison) => {
                for warning in &comparison.warnings {
                    eprintln!("warning: {}: {warning}", comparison.after.display());
                }
                comparisons.push(comparison);
            }
            Err(err) => {
                failed += 1;
                eprintln!("error: {err}");
            }
        }
    }

    let format = resolve_format(cli.format, cli.output.as_deref());
    write_comparisons(&comparisons, format, &mut io::stdout())?;
    if let Some(path) = &cli.output {
        let mut file = File::create(path).map_err(|source| Error::Write {
            path: path.clone(),
            source,
        })?;
        write_comparisons(&comparisons, format, &mut file)?;
    }
    if failed > 0 {
        eprintln!("compared {} image(s), {failed} failed", comparisons.len());
    }
    Ok(failed == 0)
}

fn resolve_format(format: Option<Format>, output: Option<&std::path::Path>) -> Format {
    if let Some(format) = format {
        return format;
    }
    match output
        .and_then(|path| path.extension())
        .and_then(|ext| ext.to_str())
    {
        Some("json") => Format::Json,
        Some("csv") => Format::Csv,
        _ => Format::Table,
    }
}

fn write_reports(
    reports: &[Report],
    metrics: &MetricSet,
    format: Format,
    out: &mut dyn Write,
) -> Result<()> {
    match format {
        Format::Table => write!(out, "{}", table(reports, metrics)).map_err(write_error),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *out, reports)
                .map_err(|err| Error::Invalid(format!("could not encode json: {err}")))?;
            writeln!(out).map_err(write_error)
        }
        Format::Csv => write_csv(reports, metrics, out),
    }
}

fn write_error(source: io::Error) -> Error {
    Error::Write {
        path: PathBuf::from("<stdout>"),
        source,
    }
}

type MetricColumn = (&'static str, bool, fn(&Report) -> Option<f64>);

enum Col {
    Path,
    Reference,
    Warnings,
    Num(&'static str, fn(&Report) -> Option<f64>),
}

fn col_header(col: &Col) -> &'static str {
    match col {
        Col::Path => "path",
        Col::Reference => "reference",
        Col::Warnings => "warnings",
        Col::Num(name, _) => name,
    }
}

fn col_cell(col: &Col, report: &Report, empty: &str) -> String {
    match col {
        Col::Path => report.path.display().to_string(),
        Col::Reference => report
            .reference
            .as_ref()
            .map(|path| path.display().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| empty.to_string()),
        Col::Warnings => report.warnings.join("; "),
        Col::Num(_, pick) => pick(report)
            .map(fmt_num)
            .unwrap_or_else(|| empty.to_string()),
    }
}

fn table_columns(metrics: &MetricSet) -> Vec<Col> {
    let mut cols = vec![Col::Path];
    if metrics.any_full_reference() {
        cols.push(Col::Reference);
    }
    let numeric: &[MetricColumn] = &[
        ("mse", metrics.mse, |r| {
            r.metrics.full_reference.and_then(|m| m.mse)
        }),
        ("rmse", metrics.rmse, |r| {
            r.metrics.full_reference.and_then(|m| m.rmse)
        }),
        ("mae", metrics.mae, |r| {
            r.metrics.full_reference.and_then(|m| m.mae)
        }),
        ("psnr", metrics.psnr, |r| {
            r.metrics.full_reference.and_then(|m| m.psnr)
        }),
        ("michelson", metrics.michelson, |r| {
            r.metrics.michelson.map(|m| m.global)
        }),
        ("michelson_block", metrics.michelson, |r| {
            r.metrics.michelson.map(|m| m.block_mean)
        }),
        ("eme", metrics.eme, |r| r.metrics.eme),
        ("gsa_deg", metrics.gsa, |r| {
            r.metrics.grayscale_angle.map(|m| m.mean_angle_deg)
        }),
        ("wgsa", metrics.gsa, |r| {
            r.metrics.grayscale_angle.map(|m| m.wgsa)
        }),
        ("hist_entropy", metrics.histogram, |r| {
            r.metrics
                .histogram
                .as_ref()
                .map(|h| h.luminance.entropy_bits)
        }),
        ("hist_mean", metrics.histogram, |r| {
            r.metrics.histogram.as_ref().map(|h| h.luminance.mean)
        }),
        ("hist_std", metrics.histogram, |r| {
            r.metrics.histogram.as_ref().map(|h| h.luminance.std_dev)
        }),
        ("hist_range", metrics.histogram, |r| {
            r.metrics
                .histogram
                .as_ref()
                .map(|h| h.luminance.dynamic_range)
        }),
        ("uicm", metrics.uicm || metrics.uiqm, |r| {
            r.metrics.underwater.and_then(|m| m.uicm).map(|m| m.uicm)
        }),
        ("uism", metrics.uism || metrics.uiqm, |r| {
            r.metrics.underwater.and_then(|m| m.uism)
        }),
        ("uiconm", metrics.uiconm || metrics.uiqm, |r| {
            r.metrics.underwater.and_then(|m| m.uiconm)
        }),
        ("uiqm", metrics.uiqm, |r| {
            r.metrics.underwater.and_then(|m| m.uiqm)
        }),
    ];
    for (name, show, pick) in numeric {
        if *show {
            cols.push(Col::Num(name, *pick));
        }
    }
    cols
}

fn table(reports: &[Report], metrics: &MetricSet) -> String {
    let cols = table_columns(metrics);
    let mut table = Table::new();
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(cols.iter().map(col_header));
    for report in reports {
        table.add_row(cols.iter().map(|col| col_cell(col, report, "-")));
    }
    table.to_string() + "\n"
}

fn write_csv(reports: &[Report], metrics: &MetricSet, out: &mut dyn Write) -> Result<()> {
    let cols = csv_columns(metrics);
    let mut writer = csv::Writer::from_writer(out);
    writer
        .write_record(cols.iter().map(col_header))
        .map_err(|err| Error::Invalid(format!("could not write csv: {err}")))?;
    for report in reports {
        let row: Vec<String> = cols.iter().map(|col| csv_cell(col, report)).collect();
        writer
            .write_record(&row)
            .map_err(|err| Error::Invalid(format!("could not write csv: {err}")))?;
    }
    writer
        .flush()
        .map_err(|err| Error::Invalid(format!("could not write csv: {err}")))?;
    Ok(())
}

fn csv_columns(metrics: &MetricSet) -> Vec<Col> {
    let mut cols = vec![
        Col::Path,
        Col::Reference,
        Col::Warnings,
        Col::Num("width", |report| Some(f64::from(report.width))),
        Col::Num("height", |report| Some(f64::from(report.height))),
    ];
    if metrics.gsa {
        cols.push(Col::Num("gsa_weight", |report| {
            report
                .metrics
                .grayscale_angle
                .map(|angle| angle.wavelet_weight)
        }));
    }
    if metrics.uicm || metrics.uiqm {
        cols.push(Col::Num("uicm_mu_rg", |report| {
            report
                .metrics
                .underwater
                .and_then(|m| m.uicm)
                .map(|m| m.mu_rg)
        }));
        cols.push(Col::Num("uicm_mu_yb", |report| {
            report
                .metrics
                .underwater
                .and_then(|m| m.uicm)
                .map(|m| m.mu_yb)
        }));
        cols.push(Col::Num("uicm_sigma_rg", |report| {
            report
                .metrics
                .underwater
                .and_then(|m| m.uicm)
                .map(|m| m.sigma_rg)
        }));
        cols.push(Col::Num("uicm_sigma_yb", |report| {
            report
                .metrics
                .underwater
                .and_then(|m| m.uicm)
                .map(|m| m.sigma_yb)
        }));
    }
    cols.extend(
        table_columns(metrics)
            .into_iter()
            .filter(|col| matches!(col, Col::Num(_, _))),
    );
    cols
}

fn csv_cell(col: &Col, report: &Report) -> String {
    match col {
        Col::Path => report.path.display().to_string(),
        Col::Reference => report
            .reference
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        Col::Warnings => report.warnings.join("; "),
        Col::Num("width", _) => report.width.to_string(),
        Col::Num("height", _) => report.height.to_string(),
        Col::Num(_, pick) => pick(report).map(fmt_full).unwrap_or_default(),
    }
}

fn write_comparisons(
    comparisons: &[Comparison],
    format: Format,
    out: &mut dyn Write,
) -> Result<()> {
    match format {
        Format::Table => write!(out, "{}", comparison_table(comparisons)).map_err(write_error),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *out, comparisons)
                .map_err(|err| Error::Invalid(format!("could not encode json: {err}")))?;
            writeln!(out).map_err(write_error)
        }
        Format::Csv => write_comparison_csv(comparisons, out),
    }
}

fn comparison_table(comparisons: &[Comparison]) -> String {
    let mut table = Table::new();
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(["file", "score", "before", "after", "change"]);
    for comparison in comparisons {
        let file = comparison.after.display().to_string();
        for change in &comparison.changes {
            table.add_row([
                file.clone(),
                change.name.to_string(),
                fmt_num(change.before),
                fmt_num(change.after),
                fmt_change(change.delta),
            ]);
        }
        if let Some(error) = comparison.full_reference {
            for (name, value) in full_reference_values(error) {
                table.add_row([
                    file.clone(),
                    name.to_string(),
                    "-".to_string(),
                    value.map(fmt_num).unwrap_or_else(|| "-".to_string()),
                    "-".to_string(),
                ]);
            }
        }
    }
    table.to_string() + "\n"
}

fn write_comparison_csv(comparisons: &[Comparison], out: &mut dyn Write) -> Result<()> {
    let mut writer = csv::Writer::from_writer(out);
    writer
        .write_record(["file", "before_file", "score", "before", "after", "change"])
        .map_err(|err| Error::Invalid(format!("could not write csv: {err}")))?;
    for comparison in comparisons {
        let file = comparison.after.display().to_string();
        let before = comparison.before.display().to_string();
        for change in &comparison.changes {
            writer
                .write_record([
                    file.as_str(),
                    before.as_str(),
                    change.name.as_str(),
                    &fmt_full(change.before),
                    &fmt_full(change.after),
                    &fmt_full(change.delta),
                ])
                .map_err(|err| Error::Invalid(format!("could not write csv: {err}")))?;
        }
        if let Some(error) = comparison.full_reference {
            for (name, value) in full_reference_values(error) {
                writer
                    .write_record([
                        file.as_str(),
                        before.as_str(),
                        name,
                        "",
                        &value.map(fmt_full).unwrap_or_default(),
                        "",
                    ])
                    .map_err(|err| Error::Invalid(format!("could not write csv: {err}")))?;
            }
        }
    }
    writer
        .flush()
        .map_err(|err| Error::Invalid(format!("could not write csv: {err}")))?;
    Ok(())
}

fn full_reference_values(error: oceanmaster::FullReference) -> [(&'static str, Option<f64>); 4] {
    [
        ("mse", error.mse),
        ("rmse", error.rmse),
        ("mae", error.mae),
        ("psnr", error.psnr),
    ]
}

fn fmt_num(value: f64) -> String {
    if value.is_infinite() && value.is_sign_positive() {
        "inf".to_string()
    } else if value.is_infinite() {
        "-inf".to_string()
    } else if value.is_nan() {
        "nan".to_string()
    } else {
        format!("{value:.4}")
    }
}

fn fmt_change(value: f64) -> String {
    if value.is_finite() && value > 0.0 {
        format!("+{value:.4}")
    } else {
        fmt_num(value)
    }
}

fn fmt_full(value: f64) -> String {
    if value.is_infinite() && value.is_sign_positive() {
        "inf".to_string()
    } else if value.is_infinite() {
        "-inf".to_string()
    } else if value.is_nan() {
        "nan".to_string()
    } else {
        format!("{value}")
    }
}
