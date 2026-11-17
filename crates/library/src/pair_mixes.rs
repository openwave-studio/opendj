//! Pair mixes: how the DJ mixed one track into another, learned from a
//! recorded set in the Mixed Set Editor. Automix plays the same pair the
//! same way: from the outgoing track's out point, with the incoming one
//! cued at its in point, for as many bars. One mix per ordered pair; a
//! newer one replaces it. A track's own Automix points (`markers` of kind
//! `automix`) are never written from here: those are set in Prep.

use rusqlite::{params, OptionalExtension};

use crate::{now, row_id, track_id, Error, Library};

pub(crate) const PAIR_MIXES_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS pair_mixes (
    out_track INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    in_track INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    out_seconds REAL NOT NULL,
    in_seconds REAL NOT NULL,
    bars REAL NOT NULL,
    style TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT '',
    created INTEGER NOT NULL,
    PRIMARY KEY (out_track, in_track)
);
";

/// How one track was mixed into another.
#[derive(Clone, Debug, PartialEq)]
pub struct PairMix {
    /// App track ids (as `Track::id`).
    pub out_track: usize,
    pub in_track: usize,
    /// Where the incoming track came in, in the outgoing track's seconds.
    pub out_seconds: f64,
    /// Where the incoming track was then, in its own seconds.
    pub in_seconds: f64,
    /// How long the mix lasted.
    pub bars: f64,
    /// The style found ("Bass swap", "Filter", ...).
    pub style: String,
    /// Where it was learned: the recording's file name.
    pub source: String,
    /// Unix seconds; set on save.
    pub created: i64,
}

impl Library {
    /// Saves a pair mix, replacing the pair's earlier one. Tracks from
    /// outside the library (the samples) can't have one: Ok(false).
    pub fn save_pair_mix(&self, m: &PairMix) -> Result<bool, Error> {
        let (Some(out), Some(inc)) = (row_id(m.out_track), row_id(m.in_track)) else {
            return Ok(false);
        };
        self.db.execute(
            "INSERT INTO pair_mixes
                (out_track, in_track, out_seconds, in_seconds, bars, style, source, created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (out_track, in_track) DO UPDATE SET
                out_seconds = excluded.out_seconds, in_seconds = excluded.in_seconds,
                bars = excluded.bars, style = excluded.style, source = excluded.source,
                created = excluded.created",
            params![
                out,
                inc,
                m.out_seconds,
                m.in_seconds,
                m.bars,
                m.style,
                m.source,
                now()
            ],
        )?;
        Ok(true)
    }

    /// The saved mix of `out_track` into `in_track` (app track ids).
    pub fn pair_mix(&self, out_track: usize, in_track: usize) -> Result<Option<PairMix>, Error> {
        let (Some(out), Some(inc)) = (row_id(out_track), row_id(in_track)) else {
            return Ok(None);
        };
        self.db
            .query_row(
                "SELECT out_seconds, in_seconds, bars, style, source, created
                   FROM pair_mixes WHERE out_track = ?1 AND in_track = ?2",
                params![out, inc],
                |r| {
                    Ok(PairMix {
                        out_track,
                        in_track,
                        out_seconds: r.get(0)?,
                        in_seconds: r.get(1)?,
                        bars: r.get(2)?,
                        style: r.get(3)?,
                        source: r.get(4)?,
                        created: r.get(5)?,
                    })
                },
            )
            .optional()
    }

    /// Forgets the pair's mix.
    pub fn delete_pair_mix(&self, out_track: usize, in_track: usize) -> Result<(), Error> {
        if let (Some(out), Some(inc)) = (row_id(out_track), row_id(in_track)) {
            self.db.execute(
                "DELETE FROM pair_mixes WHERE out_track = ?1 AND in_track = ?2",
                params![out, inc],
            )?;
        }
        Ok(())
    }

    /// The app track id of the library track at `path`, for matching the
    /// tracks a recorded set names to the library.
    pub fn track_id_at_path(&self, path: &std::path::Path) -> Result<Option<usize>, Error> {
        Ok(self
            .db
            .query_row(
                "SELECT id FROM tracks WHERE path = ?1",
                [path.to_string_lossy()],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .map(track_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library_with(paths: &[&str]) -> (Library, Vec<usize>) {
        let lib = Library::in_memory().unwrap();
        let ids = paths
            .iter()
            .map(|p| {
                lib.db
                    .execute(
                        "INSERT INTO tracks (path, title, source, added) VALUES (?1, ?1, 'scan', 0)",
                        [p],
                    )
                    .unwrap();
                lib.track_id_at_path(std::path::Path::new(p)).unwrap().unwrap()
            })
            .collect();
        (lib, ids)
    }

    fn mix(out: usize, inc: usize, out_seconds: f64) -> PairMix {
        PairMix {
            out_track: out,
            in_track: inc,
            out_seconds,
            in_seconds: 16.0,
            bars: 16.0,
            style: "Bass swap".into(),
            source: "openDJ 2026-11-16 20.00.wav".into(),
            created: 0,
        }
    }

    #[test]
    fn one_mix_per_pair_the_newest_wins() {
        let (lib, ids) = library_with(&["/m/a.mp3", "/m/b.mp3", "/m/c.mp3"]);
        let (a, b, c) = (ids[0], ids[1], ids[2]);
        assert!(lib.save_pair_mix(&mix(a, b, 180.0)).unwrap());
        assert!(lib.save_pair_mix(&mix(a, c, 200.0)).unwrap());
        assert_eq!(lib.pair_mix(a, b).unwrap().unwrap().out_seconds, 180.0);
        assert_eq!(lib.pair_mix(a, c).unwrap().unwrap().out_seconds, 200.0);
        // The other way round is another pair.
        assert_eq!(lib.pair_mix(b, a).unwrap(), None);
        lib.save_pair_mix(&mix(a, b, 190.0)).unwrap();
        let m = lib.pair_mix(a, b).unwrap().unwrap();
        assert_eq!((m.out_seconds, m.in_seconds, m.bars), (190.0, 16.0, 16.0));
        assert_eq!(m.style, "Bass swap");
        assert!(m.created > 0);
        lib.delete_pair_mix(a, b).unwrap();
        assert_eq!(lib.pair_mix(a, b).unwrap(), None);
    }

    #[test]
    fn removing_a_track_removes_its_mixes() {
        let (lib, ids) = library_with(&["/m/a.mp3", "/m/b.mp3"]);
        lib.save_pair_mix(&mix(ids[0], ids[1], 100.0)).unwrap();
        lib.db
            .execute(
                "DELETE FROM tracks WHERE id = ?1",
                [row_id(ids[1]).unwrap()],
            )
            .unwrap();
        assert_eq!(lib.pair_mix(ids[0], ids[1]).unwrap(), None);
    }

    #[test]
    fn samples_have_no_pair_mixes() {
        let (lib, ids) = library_with(&["/m/a.mp3"]);
        assert!(!lib.save_pair_mix(&mix(ids[0], 1, 100.0)).unwrap());
        assert_eq!(lib.pair_mix(ids[0], 1).unwrap(), None);
    }
}
