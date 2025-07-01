
for x in a b c d; do
    echo $x
done

for x in a b c d
do
    echo $x
done

for x in a b c d; do
    for y in 1 2 3; do
        echo $x "->" $y
    done
done
