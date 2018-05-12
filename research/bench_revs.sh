#!/bin/sh

for rev in `git log --oneline $1..$2 | cut -f 1 -d ' '`; do
    git checkout $rev
    echo starting $rev
    for rep in 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
        RUSTFLAGS="-C target-cpu=core-avx-i" time cargo bench --bin divans --features="benchmark simd" decode_context_pure_average
    done
    echo done $rev
done
