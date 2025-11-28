#!/bin/sh

{
    echo "this is a simple block"
}

false || {
    echo "this is true"
}

false && {
    echo "this should not run"
}
