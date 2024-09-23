
package main

import "core:bufio"
import "core:encoding/csv"
import "core:fmt"
import "core:io"
import "core:mem"
import "core:os"
import "core:slice"
import "core:strconv"
import "core:strings"
import "core:time"

import "core:sys/linux"
FILE :: #config(FILE, "data.csv")

main :: proc() {
	t1 := time.now()


	file := os.open(FILE, 'r') or_else panic("cannot open file")
	defer os.close(file)
  fd := transmute(linux.Fd)file

	file_len: uint
	{
        stats : linux.Stat
		if errno := linux.fstat(fd, &stats ); errno != .NONE { panic("cannot stat file") }
		file_len = stats.size
	}

  // mmap the file
	bytes_ptr, err_mmap := linux.mmap(0, file_len, {.READ}, {.SHARED}, fd)
    if err_mmap != .NONE { panic("cannot mmap") }
  bytes := slice.bytes_from_ptr(bytes_ptr, int(file_len))

	Data :: struct {
		n_samples: u64,
		min:       f64,
		max:       f64,
		sum:       f64,
	}

	// map from city name to aggregated city data
	per_city := make(map[string]Data, 1024)
	defer delete(per_city)
	defer for city in per_city {delete(city)}

	// use an arena as a temporary allocator
	arena_buffer := make([]byte, 64 * 1024) or_else panic("cannot allocate buffer")
	arena: mem.Arena
	mem.arena_init(&arena, arena_buffer)
	context.temp_allocator = mem.arena_allocator(&arena)

	n_entries := 0

    str_iterator := string(bytes)
	for line in strings.split_lines_after_iterator(&str_iterator) {
		defer free_all(context.temp_allocator)

        line := line[:len(line)-1] // remove trailing '\n'
		toks := strings.split_n(line, ";", 2, allocator = context.temp_allocator)

		if len(toks) < 2 {continue} 	// invalid line

		n_entries += 1
		city := toks[0]
		num := strconv.parse_f64(toks[1]) or_else panic("cannot parse num")

		data, found := &per_city[city]
		if found {
			data.n_samples += 1
			data.min = min(data.min, num)
			data.max = max(data.max, num)
			data.sum += num
		} else {
			city_slice := make([]u8, len(city))
			copy(city_slice, city)
			per_city[string(city_slice)] = Data {
				n_samples = 1,
				min       = num,
				max       = num,
				sum       = num,
			}
		}

		if n_entries % 100_000 == 0 {fmt.printf("\rread %d entries", n_entries)}
	}

	Entry :: struct {
		name:       string,
		using data: Data,
	}

	entries := make([dynamic]Entry)
	defer delete(entries)

	for city, data in per_city {
		append(&entries, Entry{name = city, data = data})
	}
	slice.sort_by(entries[:], proc(i, j: Entry) -> bool {return i.name < j.name})

	fmt.print("{")
	for e in entries {
		fmt.printf("%s: %f/%f/%f,", e.name, e.min, e.sum / f64(e.n_samples), e.max)
	}
	fmt.println("}")

	t2 := time.now()
	dur_secs := time.duration_seconds(time.diff(t1, t2))

	fmt.printfln(
		"read %d entries in %fs (%f entries/s)",
		n_entries,
		dur_secs,
		f64(n_entries) / dur_secs,
	)
}
