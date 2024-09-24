
package main

import "core:fmt"
import "core:math/rand"
import "core:os"
import "core:strings"

N :: #config(N, 100_000_000)
FILE :: #config(FILE, "data.csv")


main :: proc() {

	stations := make([dynamic]string)
	defer delete(stations)

	// read set of stations
	{
		stations_map := make(map[string]bool)
		stations_txt := string(
			os.read_entire_file("weather_stations.csv") or_else panic("cannot read input"),
		)
		for line in strings.split_lines_iterator(&stations_txt) {
			tokens := strings.split(line, ";")

			if len(tokens) < 2 {continue}
			stations_map[tokens[0]] = true

		}

		for station in stations_map {
			append(&stations, strings.clone(station))
		}


	}
	fmt.printfln("found %d stations", len(stations))

	// now generate the file
	fmt.printfln("opening %w…", FILE)
	out, err_os_open := os.open(FILE, flags = os.O_WRONLY | os.O_CREATE | os.O_TRUNC, mode = 0o644)
	if err_os_open != nil {
		fmt.printfln("cannot open output file: %v", err_os_open)
		panic("fatal")
	}
	defer os.close(out)

	fmt.printfln("generating %d entries…", N)
	for i in 0 ..< N {
		city := rand.choice(stations[:])
		n := rand.float32_uniform(-10., 55.)

		fmt.fprintf(out, "%s;%.1f\n", city, n)

		if i % 100_000 == 0 {fmt.printfln("generated %d entries", i)}
	}

	return
}
