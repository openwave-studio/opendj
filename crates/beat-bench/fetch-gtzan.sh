#!/bin/sh
# Downloads 30 GTZAN excerpts (the first three of each genre) and their
# hand-marked beats, for `beat-bench --dir DIR`:
#
#     crates/beat-bench/fetch-gtzan.sh ~/beat-bench/gtzan
#
# The audio is from TempoBeatDownbeat/gtzan_mini and the beats from
# CPJKU/beat_this_annotations, both at pinned commits (the same 30 excerpts
# another open-source DJ project benchmarks on, so results compare). GTZAN's
# audio is not ours to redistribute: it is only downloaded, never committed.
set -eu
dir=${1:?usage: fetch-gtzan.sh DIR}
audio_rev=a61439e86c13037011fde8e0f0743ec55c50bce3
beats_rev=c3c47fd37d3074d9f8119f18bbf460f909609f22
mkdir -p "$dir"
for genre in blues classical country disco hiphop jazz metal pop reggae rock; do
    for i in 0 1 2; do
        stem="$genre.0000$i"
        [ -s "$dir/$stem.wav" ] || curl -fsSL -o "$dir/$stem.wav" \
            "https://raw.githubusercontent.com/TempoBeatDownbeat/gtzan_mini/$audio_rev/genres/$genre/$stem.wav"
        [ -s "$dir/$stem.beats" ] || curl -fsSL -o "$dir/$stem.beats" \
            "https://raw.githubusercontent.com/CPJKU/beat_this_annotations/$beats_rev/gtzan/annotations/beats/gtzan_${genre}_0000$i.beats"
    done
done
echo "30 excerpts in $dir"
