//! Beat tracking scores, after the usual MIR evaluation measures
//! (mir_eval's `beat` module; Davies, Degara and Plumbley, "Evaluation
//! Methods for Musical Audio Beat Tracking Algorithms", 2009).

/// The window a beat counts as found in: 70 ms either side, the standard
/// for hand-marked beats.
pub const TOLERANCE: f64 = 0.07;

/// Precision, recall and F-measure of `est` against `reference` (both
/// sorted, in seconds), each reference beat matched to at most one
/// estimate within `tol`. None when there is no reference.
pub fn f_measure(reference: &[f64], est: &[f64], tol: f64) -> Option<f64> {
    if reference.is_empty() {
        return None;
    }
    if est.is_empty() {
        return Some(0.0);
    }
    let (mut i, mut j, mut hits) = (0, 0, 0usize);
    while i < reference.len() && j < est.len() {
        let d = est[j] - reference[i];
        if d.abs() <= tol {
            hits += 1;
            i += 1;
            j += 1;
        } else if d < 0.0 {
            j += 1;
        } else {
            i += 1;
        }
    }
    let p = hits as f64 / est.len() as f64;
    let r = hits as f64 / reference.len() as f64;
    Some(if hits == 0 {
        0.0
    } else {
        2.0 * p * r / (p + r)
    })
}

/// The tempo of a run of beats that plays longest: the median gap between
/// them, weighted by time (the way the app's main BPM is the tempo that runs
/// longest).
pub fn tempo(beats: &[f64]) -> f64 {
    let mut gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.is_empty() {
        return 0.0;
    }
    gaps.sort_by(f64::total_cmp);
    let half = gaps.iter().sum::<f64>() / 2.0;
    let mut run = 0.0;
    for g in &gaps {
        run += g;
        if run >= half {
            return 60.0 / g;
        }
    }
    60.0 / gaps[gaps.len() - 1]
}

/// How a detected tempo relates to the true one: right (within 4 %), at a
/// related metrical level (half, double, a third or three times), or wrong.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TempoVerdict {
    Right,
    Related(&'static str),
    Wrong,
    None,
}

pub fn tempo_verdict(reference: f64, est: f64) -> TempoVerdict {
    if est <= 0.0 || reference <= 0.0 {
        return TempoVerdict::None;
    }
    let near = |f: f64| (est / (reference * f) - 1.0).abs() <= 0.04;
    if near(1.0) {
        return TempoVerdict::Right;
    }
    for (f, name) in [
        (0.5, "half"),
        (2.0, "double"),
        (1.0 / 3.0, "third"),
        (3.0, "triple"),
        (2.0 / 3.0, "2/3"),
        (1.5, "3/2"),
    ] {
        if near(f) {
            return TempoVerdict::Related(name);
        }
    }
    TempoVerdict::Wrong
}

/// Continuity: the share of reference beats tracked correctly, where a beat
/// counts when an estimate falls within 17.5 % of the beat period of it and
/// the gap to the estimate before matches the reference period within 17.5 %
/// too (mir_eval's CMLt). AMLt is the best of that over the reference at
/// double, half (either beat) and off-beat, so a grid at another metrical
/// level still counts.
pub fn continuity(reference: &[f64], est: &[f64]) -> Option<(f64, f64)> {
    if reference.len() < 2 {
        return None;
    }
    let cml = cmlt(reference, est);
    let mids: Vec<f64> = reference.windows(2).map(|w| (w[0] + w[1]) / 2.0).collect();
    let mut double: Vec<f64> = reference
        .iter()
        .copied()
        .chain(mids.iter().copied())
        .collect();
    double.sort_by(f64::total_cmp);
    let half_a: Vec<f64> = reference.iter().copied().step_by(2).collect();
    let half_b: Vec<f64> = reference.iter().copied().skip(1).step_by(2).collect();
    let aml = [
        cml,
        cmlt(&double, est),
        cmlt(&half_a, est),
        cmlt(&half_b, est),
        cmlt(&mids, est),
    ]
    .into_iter()
    .fold(0.0, f64::max);
    Some((cml, aml))
}

fn cmlt(reference: &[f64], est: &[f64]) -> f64 {
    if reference.len() < 2 || est.len() < 2 {
        return 0.0;
    }
    let mut good = 0usize;
    for k in 1..reference.len() {
        let period = reference[k] - reference[k - 1];
        let tol = 0.175 * period;
        let i = est.partition_point(|&e| e < reference[k]);
        let nearest = [i.checked_sub(1), Some(i)]
            .into_iter()
            .flatten()
            .filter(|&i| i < est.len())
            .min_by(|&a, &b| {
                (est[a] - reference[k])
                    .abs()
                    .total_cmp(&(est[b] - reference[k]).abs())
            });
        let Some(i) = nearest else { continue };
        if i == 0 || (est[i] - reference[k]).abs() > tol {
            continue;
        }
        if ((est[i] - est[i - 1]) - period).abs() <= tol {
            good += 1;
        }
    }
    good as f64 / (reference.len() - 1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beats(bpm: f64, from: f64, n: usize) -> Vec<f64> {
        (0..n).map(|k| from + k as f64 * 60.0 / bpm).collect()
    }

    #[test]
    fn a_perfect_estimate_scores_one() {
        let r = beats(120.0, 0.5, 64);
        assert_eq!(f_measure(&r, &r, TOLERANCE), Some(1.0));
        assert_eq!(continuity(&r, &r), Some((1.0, 1.0)));
        assert_eq!(tempo_verdict(tempo(&r), 120.0), TempoVerdict::Right);
    }

    #[test]
    fn double_time_keeps_aml_but_not_cml() {
        let r = beats(120.0, 0.5, 64);
        let e = beats(240.0, 0.5, 128);
        let (cml, aml) = continuity(&r, &e).unwrap();
        assert!(cml < 0.1, "{cml}");
        assert!(aml > 0.95, "{aml}");
        assert_eq!(tempo_verdict(120.0, 240.0), TempoVerdict::Related("double"));
        let f = f_measure(&r, &e, TOLERANCE).unwrap();
        assert!((f - 2.0 / 3.0).abs() < 0.01, "{f}");
    }

    #[test]
    fn off_beat_scores_nothing_at_cml() {
        let r = beats(120.0, 0.5, 64);
        let e = beats(120.0, 0.75, 64);
        assert_eq!(f_measure(&r, &e, TOLERANCE), Some(0.0));
        let (cml, aml) = continuity(&r, &e).unwrap();
        assert_eq!(cml, 0.0);
        assert!(aml > 0.95);
    }
}
