#!/usr/bin/env python

from typing import List

print("(declare-sort u 0)")

declared = set({})
gen_ = 0

def declare(v: str, sort = "u"):
    if v not in declared:
        declared.add(v)
        print(f"(declare-fun {v} () {sort})")

def new_guard() -> str:
    "generate new literal"
    global gen_
    x = f"p_{gen_}"
    gen_ += 1
    return x


def new_point() -> str:
    "generate new middle point"
    global gen_
    x = f"z_{gen_}"
    gen_ += 1
    return x

def gen_diamond(x: str, y: str, guard, depth: int):
    low = new_point()
    high = new_point()
    declare(low)
    declare(high)
    if depth <= 1:
        print(f"(assert (or {guard} (and (= {x} {low}) (= {low} {y})) (and (= {x} {high}) (= {high} {y}))))")
    else:
        p = new_guard()
        declare(p, "Bool")
        gen_diamond(x, low, p, depth-1)
        gen_diamond(low, y, p, depth-1)
        gen_diamond(x, high, f"(not {p})", depth-1)
        gen_diamond(high, y, f"(not {p})", depth-1)

def gen(vars: List[str], depth: int):
    for v in vars:
        declare(v)
    for i in range(0, len(vars)-1):
        x = vars[i]
        y = vars[i+1]
        gen_diamond(x,y,"false",depth)


top_vars = [f"x{i}" for i in range(0, 50)]

gen(top_vars, 3)
print(f"(assert (not (= {top_vars[0]} {top_vars[-1]})))")
print("(check-sat)")


