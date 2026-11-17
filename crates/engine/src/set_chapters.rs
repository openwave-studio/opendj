//! A recorded set's tracklist as chapters, for where the mix goes next: a
//! Mixcloud tracklist, YouTube chapters for the description, and a `.cue`
//! sheet. The times follow the quick fixes (see [`crate::set_fixes`]): a
//! cut or trim moves everything after it earlier.

use crate::set_fixes::Edits;
use crate::set_timeline::{CueTrack, SetTimeline};

/// YouTube's rules for chapters in a description: the first at 0:00, at
/// least three, each at least ten seconds long.
pub const YOUTUBE_MIN_CHAPTERS: usize = 3;
pub const YOUTUBE_MIN_SECONDS: f64 = 10.0;

/// One track of the set, from when it was heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    /// Seconds into the recording (or the edited file, once moved).
    pub start: f64,
    pub artist: String,
    pub title: String,
}

impl Chapter {
    /// `Artist - Title`, or whichever of them there is.
    pub fn name(&self) -> String {
        match (self.artist.trim(), self.title.trim()) {
            ("", t) => t.to_string(),
            (a, "") => a.to_string(),
            (a, t) => format!("{a} - {t}"),
        }
    }
}

/// The set's chapters from its timeline: the first track from when it was
/// first heard, then each incoming track from the start of its transition.
pub fn from_timeline(t: &SetTimeline) -> Vec<Chapter> {
    let chapter = |track: usize, start: f64| Chapter {
        start,
        artist: t.tracks[track].artist.clone(),
        title: t.tracks[track].title.clone(),
    };
    let mut out = Vec::new();
    if let Some(c) = t.clips.first() {
        out.push(chapter(c.track, c.start));
    }
    for x in &t.transitions {
        out.push(chapter(x.to_track, x.start));
    }
    out
}

/// The chapters of a recording without a control log, from its cue sheet.
pub fn from_cue(tracks: &[CueTrack]) -> Vec<Chapter> {
    tracks
        .iter()
        .map(|c| Chapter {
            start: c.start,
            artist: c.performer.clone(),
            title: c.title.clone(),
        })
        .collect()
}

/// The chapters moved to where they land after the fixes: one that starts
/// in a stretch taken out starts where the stretch was; one trimmed off
/// the end is dropped.
pub fn follow(chapters: &[Chapter], edits: &Edits, length: f64) -> Vec<Chapter> {
    let end = edits.map_time(length - 1e-6);
    let mut out: Vec<Chapter> = Vec::new();
    for c in chapters {
        // Inside a cut or before a trim: from the first moment kept after.
        let start = edits
            .next_kept(c.start)
            .filter(|t| *t < length)
            .and_then(|t| edits.map_time(t));
        let Some(start) = start else { continue };
        if end.is_some_and(|e| start >= e) {
            continue;
        }
        if let Some(last) = out.last_mut() {
            // Two chapters landing together: the later track wins.
            if start - last.start < 1e-6 {
                *last = Chapter { start, ..c.clone() };
                continue;
            }
        }
        out.push(Chapter { start, ..c.clone() });
    }
    out
}

/// `0:00` or `1:02:03`.
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// `0:00 Artist - Title` per track, for Mixcloud and show notes.
pub fn tracklist(chapters: &[Chapter]) -> String {
    chapters
        .iter()
        .map(|c| format!("{} {}\n", clock(c.start), c.name()))
        .collect()
}

/// Why the chapters don't make YouTube chapters as they are.
#[derive(Clone, Debug, PartialEq)]
pub enum YoutubeNote {
    /// Fewer than three chapters: YouTube shows none.
    TooFew,
    /// Chapters shorter than ten seconds were joined to the one before.
    Joined(usize),
}

/// YouTube chapters for a description, and what had to change to meet
/// YouTube's rules. The first is moved to 0:00; chapters under ten
/// seconds are joined to the one before.
pub fn youtube(chapters: &[Chapter], length: f64) -> (String, Vec<YoutubeNote>) {
    let mut kept: Vec<Chapter> = Vec::new();
    let mut joined = 0;
    for c in chapters {
        if let Some(last) = kept.last() {
            if c.start - last.start < YOUTUBE_MIN_SECONDS {
                joined += 1;
                continue;
            }
        }
        kept.push(c.clone());
    }
    if let Some(first) = kept.first_mut() {
        first.start = 0.0;
    }
    // The last one needs ten seconds before the end too.
    while kept.len() > 1 && length - kept[kept.len() - 1].start < YOUTUBE_MIN_SECONDS {
        kept.pop();
        joined += 1;
    }
    let mut notes = Vec::new();
    if joined > 0 {
        notes.push(YoutubeNote::Joined(joined));
    }
    if kept.len() < YOUTUBE_MIN_CHAPTERS {
        notes.push(YoutubeNote::TooFew);
    }
    // Hours on every line once the set runs past one, as YouTube expects
    // the same form throughout.
    let long = length >= 3600.0;
    let text = kept
        .iter()
        .map(|c| {
            let at = if long && c.start < 3600.0 {
                let s = c.start as u64;
                format!("0:{:02}:{:02}", s / 60, s % 60)
            } else {
                clock(c.start)
            };
            format!("{at} {}\n", c.name())
        })
        .collect();
    (text, notes)
}

/// A `.cue` sheet for the file `audio` (its name), titled `title`.
pub fn cue_sheet(chapters: &[Chapter], title: &str, audio: &str) -> String {
    let q = |s: &str| s.replace('"', "'");
    let mut s = String::new();
    s.push_str("REM COMMENT \"Made with openDJ\"\n");
    s.push_str("PERFORMER \"openDJ\"\n");
    s.push_str(&format!("TITLE \"{}\"\n", q(title)));
    let kind = if audio.to_ascii_lowercase().ends_with(".mp3") {
        "MP3"
    } else {
        "WAVE"
    };
    s.push_str(&format!("FILE \"{}\" {kind}\n", q(audio)));
    for (n, c) in chapters.iter().enumerate() {
        let frames = (c.start.max(0.0) * 75.0).round() as u64;
        s.push_str(&format!("  TRACK {:02} AUDIO\n", n + 1));
        s.push_str(&format!("    TITLE \"{}\"\n", q(&c.title)));
        s.push_str(&format!("    PERFORMER \"{}\"\n", q(&c.artist)));
        s.push_str(&format!(
            "    INDEX 01 {:02}:{:02}:{:02}\n",
            frames / 75 / 60,
            frames / 75 % 60,
            frames % 75
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set_fixes::Fix;
    use crate::set_timeline::read_cue;

    fn ch(start: f64, title: &str) -> Chapter {
        Chapter {
            start,
            artist: "Artist".into(),
            title: title.into(),
        }
    }

    #[test]
    fn chapters_come_from_the_timeline() {
        let t = crate::set_timeline::tests::bass_swap();
        let c = from_timeline(&t);
        assert_eq!(c, vec![ch(1.0, "A"), ch(30.0, "B")]);
        assert_eq!(tracklist(&c), "0:01 Artist - A\n0:30 Artist - B\n");
    }

    #[test]
    fn chapters_follow_cuts_and_trims() {
        let chapters = [ch(5.0, "A"), ch(200.0, "B"), ch(320.0, "C"), ch(590.0, "D")];
        let edits = Edits {
            fixes: vec![
                Fix::Trim {
                    start: 10.0,
                    end: 580.0,
                    fade_in: 2.0,
                    fade_out: 5.0,
                },
                Fix::Cut {
                    start: 300.0,
                    end: 330.0,
                },
            ],
        };
        let moved = follow(&chapters, &edits, 600.0);
        assert_eq!(moved.len(), 3, "D was trimmed off: {moved:?}");
        assert_eq!(moved[0].start, 0.0, "A starts with the trim");
        assert_eq!(moved[1].start, 190.0);
        // C started inside the cut: it starts at the join.
        assert!(
            (moved[2].start - (300.0 - 10.0)).abs() < 0.2,
            "{:?}",
            moved[2]
        );
    }

    #[test]
    fn youtube_chapters_follow_youtubes_rules() {
        let chapters = [ch(4.0, "A"), ch(60.0, "B"), ch(65.0, "C"), ch(300.0, "D")];
        let (text, notes) = youtube(&chapters, 600.0);
        assert_eq!(text, "0:00 Artist - A\n1:00 Artist - B\n5:00 Artist - D\n");
        assert_eq!(notes, vec![YoutubeNote::Joined(1)]);
        let (_, notes) = youtube(&chapters[..2], 600.0);
        assert_eq!(notes, vec![YoutubeNote::TooFew]);
        // Past an hour, every line has hours.
        let (text, _) = youtube(&[ch(0.0, "A"), ch(1800.0, "B"), ch(3700.0, "C")], 4000.0);
        assert_eq!(
            text,
            "0:00:00 Artist - A\n0:30:00 Artist - B\n1:01:40 Artist - C\n"
        );
    }

    #[test]
    fn a_cue_sheet_reads_back() {
        let chapters = [ch(0.0, "A"), ch(61.2, "B \"two\"")];
        let text = cue_sheet(&chapters, "My set", "My set.mp3");
        assert!(text.contains("FILE \"My set.mp3\" MP3"));
        let back = read_cue(&text);
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].title, "B 'two'");
        assert!((back[1].start - 61.2).abs() < 1e-9);
        assert_eq!(from_cue(&back)[1].artist, "Artist");
    }
}
