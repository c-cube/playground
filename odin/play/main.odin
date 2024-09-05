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

mk_int_ref :: proc() -> ^IntRef {

	ActualIntRef :: struct {
		using ref: IntRef,
		r:         i64,
	}

	incr_int_ref :: proc(r: rawptr) {
		x := (^ActualIntRef)(r)
		x.r += 1
	}

	decr_int_ref :: proc(r: rawptr) {
		x := (^ActualIntRef)(r)
		x.r -= 1
	}

	get_int_ref :: proc(r: rawptr) -> i64 {
		x := (^ActualIntRef)(r)
		return x.r
	}

	@(static)
	int_ref_procs: Procs = {
		incr = incr_int_ref,
		decr = decr_int_ref,
		get  = get_int_ref,
	}

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
