package main

import "core:fmt"

Procs :: struct {
	incr: proc(_: rawptr),
	decr: proc(_: rawptr),
	get:  proc(_: rawptr) -> i64,
}

IntRef :: struct {
	using procs: ^Procs,
}

ActualIntRef :: struct {
	using ref: IntRef,
	r:         i64,
}

@(private)
incr_int_ref :: proc(r: rawptr) {
	x := (^ActualIntRef)(r)
	x.r += 1
}

@(private)
decr_int_ref :: proc(r: rawptr) {
	x := (^ActualIntRef)(r)
	x.r -= 1
}

@(private)
get_int_ref :: proc(r: rawptr) -> i64 {
	x := (^ActualIntRef)(r)
	return x.r
}

int_ref_procs: Procs = {
	incr = incr_int_ref,
	decr = decr_int_ref,
	get  = get_int_ref,
}


mk_int_ref :: proc() -> ^IntRef {
	r := new(ActualIntRef)
	r^ = {
		r = 0,
		ref = {procs = &int_ref_procs},
	}
	return &r.ref
}

main :: proc() {
	r := mk_int_ref()
	fmt.printfln("r.get=%d", r->get())
	r->incr()
	fmt.printfln("r.get=%d", r->get())
}
