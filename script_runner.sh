#!/bin/sh
DIR="$(mktemp -d)"
for t in test_scripts/*; do
    echo "running $t"
    /bin/sh "$t" > $DIR/a
    ./target/debug/slush "$t" > $DIR/b
    if ! diff $DIR/a $DIR/b; then
        echo "Error running $t, /bin/sh and slush do no agree!"
        echo "expected:" && cat $DIR/a
        echo "found:" && cat $DIR/b
        hexdump -C $DIR/a
        hexdump -C $DIR/b
        rm -rf $DIR/*
        exit 1
    fi
    rm -rf $DIR/*
done

echo "All tests passed successfully"
