X='c'

while [ ${X} != 'cccccccc' ]; do
    echo $X
    while [ ${X} != 'cccc' ]; do
        X=$X$X
    done
    X=$X$X
    echo $X
done

echo 'trying until'
X='c'

until [ ${X} = 'cccccccc' ]; do
     echo $X
    X=$X$X
done

echo $X
