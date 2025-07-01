#!/bin/sh


duck() {
    quack() {
        echo "quack quack"
    }
    echo what
    echo "from duck:" $1
    quack
}

score() {
    echo "haha" $1 $2
    duck $2
}

# echo hello $1 $2 # how we handle empty vars is wrong
duck "goose"
duck "goose" "moose"
score "moose" "mouse"
quack

