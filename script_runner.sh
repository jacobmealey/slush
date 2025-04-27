#!/bin/sh
for t in test_scripts/*; do
    echo "running $t"
    if [ "$(/bin/sh "$t")" != "$(./target/debug/slush "$t")" ]; then
        echo "Error running $t, /bin/sh and slush do no agree!"
        exit 1
    fi
done

echo "All tests passed succesfully"
