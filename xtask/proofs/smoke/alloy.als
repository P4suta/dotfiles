module verification_smoke

one sig Item {}

assert Holds { some Item }
assert Breaks { no Item }

check Holds for 1
run Exists { some Item } for 1
check Breaks for 1
