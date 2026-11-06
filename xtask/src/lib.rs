const CALLER_TONE_PERIOD: usize = 16;
const CALLER_AUDIBLE_SAMPLES: usize = 16000;
const BROWSER_TONE_PERIOD: usize = 20;
const FIXTURE_SAMPLES: usize = 32000;
const HOLD_AUDIBLE_SAMPLES: usize = 1600;
const SAMPLE_RATE: u32 = 8000;
const SAMPLE_BITS: u16 = 16;
const TONE_AMPLITUDE: i16 = 8316;

use std::path::Path;
pub mod admin_http;
pub mod assurance;
pub mod asterisk;
pub mod bundle;
pub mod check;
pub mod fault_catalog;
pub mod manager;
pub mod operations;
pub mod package;
pub mod provisioning;
pub mod sip_fixture;
pub mod update_flow;

pub fn write_assets(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    for (name, period, audible) in [
        ("caller", CALLER_TONE_PERIOD, CALLER_AUDIBLE_SAMPLES),
        ("browser", BROWSER_TONE_PERIOD, FIXTURE_SAMPLES),
        ("hold", CALLER_TONE_PERIOD, HOLD_AUDIBLE_SAMPLES),
    ] {
        let specification = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: SAMPLE_BITS,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer =
            hound::WavWriter::create(directory.join(format!("{name}.wav")), specification)?;
        for index in 0..FIXTURE_SAMPLES {
            let sample: i16 = if index >= audible {
                0
            } else if index % period < period / 2 {
                TONE_AMPLITUDE
            } else {
                -TONE_AMPLITUDE
            };
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
    }
    Ok(())
}
