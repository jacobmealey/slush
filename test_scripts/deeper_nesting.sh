#!/bin/sh

print_n() {
    i=$1
    echo $1 $2
    echo "$1 > 0" | bc
    echo starting condition $(echo $i '> 0' | bc)
    while [ $(echo $i '> 0' | bc) -ne 0 ]; do
        echo $2
        echo "in the loop " $(echo $i ' > 0' | bc)
        i=$(echo $i ' - 1' | bc)
        echo "i is: " $i
    done
}

print_n 4 hi
