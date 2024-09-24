#!/usr/bin/python

import dataclasses, time


@dataclasses.dataclass
class Data:
    n_samples: int = 0
    sum: float = 0
    min: float = 0
    max: float = 0


def parse(file: str) -> None:
    cities = {}

    t_start = time.monotonic_ns()
    n_entries = 0
    with open(file, "r") as f:
        for line in f.readlines():
            toks = line.strip().split(";")
            if len(toks) < 2:
                continue

            n_entries += 1
            city = toks[0]
            value = float(toks[1])

            d = cities.get(city)
            if d is None:
                d = Data()
                cities[city] = d

            d.n_samples += 1
            d.sum += value
            d.min = min(d.min, value)
            d.max = max(d.max, value)

            if n_entries % 100_000 == 0:
                print(f"\rparsed {n_entries} entries", flush=True, end="")

    cities_l = sorted(
        list((city, d.min, d.sum / d.n_samples, d.max) for (city, d) in cities.items())
    )
    print(cities_l)

    t = (time.monotonic_ns() - t_start) * 1e-9
    print(f"parsed {n_entries} in {t}s ({n_entries / t} entries/s)")


if __name__ == "__main__":
    parse("data.csv")
