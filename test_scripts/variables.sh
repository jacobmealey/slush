#!/bin/sh

VAR=foo
FOO=bar

echo ${VAR}
echo ${FOO}
echo ${SOME:-default}
echo ${SOME:=peple}
echo $SOME

echo "$SOME$SOME" "$FOO" hello "  $VAR" 

