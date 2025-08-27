#!/bin/sh

VAR=foo
FOO=bar

echo ${VAR}
echo ${FOO}
echo ${SOME:-default}
echo ${SOME:=peple}
echo $SOME

echo "$SOME$SOME" "$FOO" hello "  $VAR" 

echo "Hello, ${USER}"
echo "I am, `whoami`; or you may call me $(whoami)"
echo "Hello $SOME$SOME"
