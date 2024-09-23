
package main

import "core:bufio"
import "core:encoding/csv"
import "core:fmt"
import "core:io"
import "core:os"
import "core:slice"
import "core:strconv"
import "core:strings"

FILE :: #config(FILE, "weather_stations.csv")

main :: proc() {
	file := os.open(FILE, 'r') or_else panic("cannot open file")
	reader := os.stream_from_handle(file)
	defer os.close(file)

	raw_reader := os.stream_from_handle(file)

	//reader : bufio.Reader
	//bufio.reader_init(&reader, raw_reader)
	//defer bufio.reader_destroy(&reader)

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


	n_entries := 0
	for bufio.scanner_scan(&scanner) {
		defer free_all(context.temp_allocator)
		line := bufio.scanner_text(&scanner)

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
	}

	fmt.printfln("read %d entries", n_entries)

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
}
