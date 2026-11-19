//! Deck waveforms for controller scripts.
//!
//! Some controllers draw the track on their own screens: the DDJ-FLX10's
//! jog wheels show a scrolling waveform. Mixxx scripts can't reach the
//! waveform, so mappings that wanted it had to read Mixxx's analysis files
//! from a separate program. Here the app tells this crate where to find
//! each deck's waveform ([`set_source`]), and scripts ask for it with
//! `engine.getWaveform(group, perSecond)`.

use std::sync::{Arc, RwLock};

/// One slice of a deck's waveform: the peak of the low, mid and high bands
/// and of the whole signal, each 0 to 255 on a square-root scale.
pub type Slice = [u8; 4];

/// Gives the waveform of the track on a deck (0 for deck 1), `per_second`
/// slices a second: empty with no track loaded or before its analysis has
/// started.
pub type Source = Arc<dyn Fn(usize, f64) -> Vec<Slice> + Send + Sync>;

static SOURCE: RwLock<Option<Source>> = RwLock::new(None);

/// Where scripts' `engine.getWaveform` reads from; set once by the app.
pub fn set_source(source: Option<Source>) {
    *SOURCE.write().unwrap() = source;
}

/// The bytes `engine.getWaveform` gives for `group` ("[Channel2]"): four a
/// slice, empty for a group that isn't a deck.
pub(crate) fn for_group(group: &str, per_second: f64) -> Vec<u8> {
    let deck = group
        .strip_prefix("[Channel")
        .and_then(|r| r.strip_suffix(']'))
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|&n| n >= 1);
    let (Some(deck), true) = (deck, per_second.is_finite() && per_second > 0.0) else {
        return Vec::new();
    };
    let source = SOURCE.read().unwrap().clone();
    source
        .map_or_else(Vec::new, |s| s(deck - 1, per_second.min(10_000.0)))
        .concat()
}

/// A waveform summary's finest level, `rate` bins a second, each the peak of
/// the low, mid, high bands and whole signal followed by their RMS (the
/// engine's `WaveformBin`), resampled to `per_second` slices a second, each
/// the loudest peak of its part.
pub fn resample(finest: &[[u8; 8]], rate: f64, per_second: f64) -> Vec<Slice> {
    if finest.is_empty() || !(rate > 0.0 && per_second > 0.0) {
        return Vec::new();
    }
    let seconds = finest.len() as f64 / rate;
    let n = (seconds * per_second).floor() as usize;
    (0..n)
        .map(|i| {
            let from = ((i as f64 / per_second) * rate) as usize;
            let to = ((((i + 1) as f64 / per_second) * rate) as usize).max(from + 1);
            let part = &finest[from.min(finest.len() - 1)..to.min(finest.len())];
            std::array::from_fn(|k| part.iter().map(|b| b[k]).max().unwrap_or(0))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_to_the_loudest_peak_of_each_part() {
        // 1000 bins a second, two seconds: quiet, with one loud bin.
        let mut finest = vec![[10, 20, 30, 40, 1, 2, 3, 4]; 2000];
        finest[1500] = [200, 0, 0, 210, 0, 0, 0, 0];
        let w = resample(&finest, 1000.0, 150.0);
        assert_eq!(w.len(), 300);
        assert_eq!(w[0], [10, 20, 30, 40]);
        // Second 1.5 lands in slice 225.
        assert_eq!(w[225], [200, 20, 30, 210]);
        assert_eq!(w[226], [10, 20, 30, 40]);
        assert!(resample(&[], 1000.0, 150.0).is_empty());
    }

    #[test]
    fn scripts_get_four_bytes_a_slice_by_deck() {
        set_source(Some(Arc::new(|deck, per_second| {
            vec![[deck as u8, 1, 2, 3]; per_second as usize]
        })));
        assert_eq!(for_group("[Channel2]", 2.0), vec![1, 1, 2, 3, 1, 1, 2, 3]);
        assert!(for_group("[Master]", 2.0).is_empty());
        assert!(for_group("[Channel0]", 2.0).is_empty());
        assert!(for_group("[Channel1]", f64::NAN).is_empty());
        set_source(None);
        assert!(for_group("[Channel1]", 2.0).is_empty());
    }
}
