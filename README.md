# oceanmaster

Image-quality metrics for underwater photos, as a Rust library and a command-line tool.

Score one image, or a folder of them. Add a reference when you want error metrics. The underwater scores need no reference.

## Install

```bash
cargo install --path .
oceanmaster --help
```

Library-only dependents can skip the CLI:

```toml
oceanmaster = { version = "0.1", default-features = false }
```

The default feature `cli` builds the `oceanmaster` binary.

## Command line

```bash
oceanmaster -i reef.jpg
oceanmaster -i ./photos -o report.csv
oceanmaster -i enhanced.png -r original.png
oceanmaster -i enhanced.png --before original.png
oceanmaster -i ./enhanced --before ./original --metrics underwater
oceanmaster -i ./enhanced -r ./original --metrics underwater,psnr
oceanmaster -i reef.jpg --uicm-alpha-r 0.2 --c3 1.0
```

`--input` is a file or a folder. Folders are not recursive unless you pass `--recursive`.

`--reference` is the partner for pixel error (MSE, RMSE, MAE, PSNR). `--before` is the unprocessed image: each no-reference score is printed for that image and for `--input`, plus the change. Two files are paired even if their names differ. Two folders are matched by the same relative path, then by the same file stem if the extension differs (`frame.png` can match `frame.jpg`). A file with no match is still scored on its own when you only passed `--reference`. A `--before` comparison skips a file that has no match.

`--format` is `table` (default), `json`, or `csv`. If you omit it and `--output` ends in `.json` or `.csv`, that extension is used. Histogram bin counts are included only in JSON.

`--metrics` accepts a preset or a comma-separated list:

| Value | What it runs |
| --- | --- |
| `auto` | No-reference scores, plus MSE, RMSE, MAE, and PSNR when `--reference` is set. This is the default. |
| `all` | Every score that can run with the given inputs. |
| `underwater` | UICM, UISM, UIConM, UIQM. |
| `noref` | Michelson, EME, grayscale angle, histogram, and the underwater scores. |
| `reference` | MSE, RMSE, MAE, PSNR. |
| `uicm,psnr` | Any mix of names. |

Accepted images: PNG, JPEG, WebP, TIFF, BMP, GIF (first frame), and TGA. They are converted to 8-bit RGB before scoring.

## Before and after

`--input` is the processed image. `--before` is the same picture before processing.

```bash
oceanmaster -i enhanced.png --before original.png
oceanmaster -i ./enhanced --before ./original -o changes.csv
```

Each no-reference score is computed twice. The change is `after - before`. A positive change means the score went up. For UIQM, UISM, UIConM, and UICM, up is the direction the paper treats as an improvement.

```text
file            score   before   after    change
enhanced.png    uiqm    0.8123   1.5402   +0.7279
enhanced.png    uicm   -0.4200   1.1020   +1.5220
enhanced.png    uism    1.2044   2.0180   +0.8136
```

The two images do not have to be the same size for this comparison. If you also ask for MSE or PSNR, those are computed against `--reference` when you pass one, otherwise against `--before`. Different sizes skip the pixel-error numbers and still print the score changes.

`compare_scores` does the same subtraction in the library, given two `Metrics` values. `compare_files` loads the pair.

## Metrics

No reference needed:

| Score | What it answers |
| --- | --- |
| UICM | Is the color stuck on green or blue? |
| UISM | How sharp are the edges? |
| UIConM | How much local contrast is there? |
| UIQM | One combined underwater score: `c1*UICM + c2*UISM + c3*UIConM`. |
| Michelson | `(max - min) / (max + min)`, globally and by tile. |
| EME | Enhancement measure on luminance. |
| Weighted grayscale angle | Edge-profile sharpness, weighted by Haar detail energy. |
| Histogram | Mean, variance, skewness, excess kurtosis, dynamic range, and Shannon entropy, for luminance and for each RGB channel. |

Needs a reference of the same size:

| Score | Definition |
| --- | --- |
| MSE, RMSE, MAE | Error over every RGB sample. |
| PSNR | `10 * log10(255² / MSE)`. Identical images report `inf`. |

A higher UIQM, under the chosen weights, is a better image. A 10% rise is about the step a person can see, according to the paper below.

### Weights you can change

| Flag | Default | Meaning |
| --- | --- | --- |
| `--c1` | `0.0282` | UIQM weight on colorfulness. |
| `--c2` | `0.2953` | UIQM weight on sharpness. |
| `--c3` | `3.5753` | UIQM weight on contrast. |
| `--uicm-alpha-l` | `0.1` | Drop the lowest 10% of the RG and YB samples. |
| `--uicm-alpha-r` | `0.1` | Drop the highest 10%. Raise this for bubbles or light rays. |
| `--uicm-mean-weight` | `-0.0268` | Weight on the color-cast term. |
| `--uicm-var-weight` | `0.1586` | Weight on the color-spread term. |
| `--block-size` | `8` | Tile side for EME, block Michelson, UISM, and UIConM. |

UICM has two learned weights, not six. Color is measured in the opponent plane (`RG = R - G`, `YB = (R + G) / 2 - B`). After the tails are trimmed:

```text
UICM = -0.0268 * sqrt(μ_RG² + μ_YB²) + 0.1586 * sqrt(σ_RG² + σ_YB²)
```

The first term is how far the color sits from neutral, so its weight is negative. The second term is the chroma spread, so its weight is positive. `alpha_l` and `alpha_r` only choose the trim.

The same knobs are fields on `Config` in the library. Channel weights for UISM, the Sobel edge floor, and the PLIP gamma for UIConM are library parameters and stay at the paper defaults unless you set them there.

## Library

```rust
use image::{Rgb, RgbImage};
use oceanmaster::{evaluate, Config, MetricSet};

let image = RgbImage::from_pixel(16, 16, Rgb([40, 120, 90]));
let mut config = Config::default();
config.metrics = MetricSet::underwater();
let report = evaluate(&image, None, &config)?;
let uiqm = report.underwater.unwrap().uiqm.unwrap();
```

Single metrics are also public: `mse`, `psnr`, `uicm`, `uism`, `uiconm`, `uiqm`, and the rest of `oceanmaster::metrics`. `load` and `discover` read files and match a reference folder by name.

```bash
cargo doc --no-deps --open
```

The module docs state where a printed equation is undefined on real pixels, and which defined reading is used. UISM ignores non-edge pixels so `log(max/min)` does not divide by zero. UIConM reports the absolute mean of `r * ln(r)`, because the outer PLIP multiply is not defined for a negative sum.

## Citation

Underwater formulas follow:

K. Panetta, C. Gao, and S. Agaian, "Human-Visual-System-Inspired Underwater Image Quality Measures," *IEEE Journal of Oceanic Engineering*, vol. 41, no. 3, pp. 541–551, 2016. DOI: [10.1109/JOE.2015.2469915](https://doi.org/10.1109/JOE.2015.2469915).

The weighted grayscale angle follows the Hou and Weidemann edge-profile method cited there.
