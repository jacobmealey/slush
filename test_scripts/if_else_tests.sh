#!/usr/local/bin/slush
# echo hello world

# CC=gcc

if true; then
    if true; then
        echo "okay"
    elif false; then
        echo "not okay"
    else
        echo "nope"
    fi
    printf "this is a test\n"
fi
if [ -f afile ]; then
    echo '===== we are in da if statement ====='
    echo 'another statement'
elif [ -f bfile ]; then
    echo 'elif elif elif'
else
    echo 'we are in the else statement'
fi

echo 'hello' \
    'old world'


echo $CC
echo hello
