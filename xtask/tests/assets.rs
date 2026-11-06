#[test]
fn audio_fixtures_have_telephony_format_and_distinct_carrier_frequencies() {
    let directory = tempfile::tempdir().unwrap();
    xtask::write_assets(directory.path()).unwrap();
    for (name, crossings) in [("caller", 1000), ("browser", 1600), ("hold", 100)] {
        let reader = hound::WavReader::open(directory.path().join(format!("{name}.wav"))).unwrap();
        assert_eq!(reader.spec().sample_rate, 8000);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 16);
        let samples: Vec<i16> = reader.into_samples().collect::<Result<_, _>>().unwrap();
        assert_eq!(samples.len(), 32000);
        let measured = samples
            .windows(2)
            .filter(|pair| pair[0] < 0 && pair[1] > 0)
            .count();
        assert!(
            (crossings - 1..=crossings).contains(&measured),
            "{name}: {measured}"
        );
        if name == "browser" {
            assert!(samples.iter().all(|sample| sample.unsigned_abs() == 8316));
        } else {
            assert!(samples.contains(&0));
        }
    }
}
