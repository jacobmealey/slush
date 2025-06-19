#!/bin/sh
for t in test_scripts/*; do
    echo "running $t"
    a="$(/bin/sh "$t")"
    b="$(./target/debug/slush "$t")"
    if [ "$a" !=  "$b" ]; then
        echo "Error running $t, /bin/sh and slush do no agree!"
        echo "expected:\n" $a
        echo "found:\n" $b
        exit 1
    fi
done

echo "All tests passed successfully"
