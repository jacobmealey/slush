#!/bin/sh

positionals() {
    echo $@
    echo $*
}

counters() {
    echo $#
}


positionals 1 2 3 4
positionals "hello" old friend
positionals "hello" old $# 

counters 1 2 3
counters 1    2 3 how
