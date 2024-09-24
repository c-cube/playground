
package main

import "core:bufio"
import "core:fmt"
import "core:os"
import "core:slice"
import "core:strconv"
import "core:strings"
import "core:time"

FILE :: #config(FILE, "data.csv")

main :: proc() {
	t1 := time.now()

	file := os.open(FILE, 'r') or_else panic("cannot open file")
	defer os.close(file)

	raw_reader := os.stream_from_handle(file)

	scan_buf := make([]u8, 4 * 1024 * 1024) or_else panic("cannot allocate buffer")
	defer delete(scan_buf)

	scanner: bufio.Scanner = {
		split = bufio.scan_lines,
	}
	bufio.scanner_init_with_buffer(&scanner, raw_reader, scan_buf)
	defer bufio.scanner_destroy(&scanner)

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
	//arena_buffer := make([]byte, 64 * 1024) or_else panic("cannot allocate buffer")
	//arena: mem.Arena
	//mem.arena_init(&arena, arena_buffer)
	//context.temp_allocator = mem.arena_allocator(&arena)

	n_entries := 0
	for bufio.scanner_scan(&scanner) {
		// defer free_all(context.temp_allocator)
		line := bufio.scanner_text(&scanner)

		split := 0
		for split < len(line) {
			if line[split] == ';' {break}
			split += 1
		}
		if split == len(line) {continue} 	// invalid line

		n_entries += 1
		city := line[:split]
		// fmt.printfln("line=%w, split=%d", line, split)
		num, num_ok := strconv.parse_f64(line[split + 1:])
		if !num_ok {
			fmt.printfln("failed to read line %w (num=%w)", line, line[split + 1:])
			panic("cannot parse num")
		}

		data, found := &per_city[city]
		if found {
			data.n_samples += 1
			data.min = min(data.min, num)
			data.max = max(data.max, num)
			data.sum += num
		} else {
			city2 := strings.clone(city)
			per_city[city2] = Data {
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
	for e, _idx in entries {
		if _idx > 0 {fmt.print(",")}
		fmt.printf("%s:%f/%f/%f", e.name, e.min, e.sum / f64(e.n_samples), e.max)
	}
	fmt.println("}")

	t2 := time.now()
	dur_secs := time.duration_seconds(time.diff(t1, t2))

	fmt.printfln(
		"read %d entries (for %d cities) in %fs (%f entries/s)",
		n_entries,
		len(entries),
		dur_secs,
		f64(n_entries) / dur_secs,
	)
}
