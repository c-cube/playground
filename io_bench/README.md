
depends on `hyperfine` and `dd`

```
$ make
### buffered                                                                                                                                                                                                     [41/90]
hyperfine --warmup 2 './run buf f_500M'
Benchmark 1: ./run buf f_500M
  Time (mean ± σ):      93.4 ms ±   4.4 ms    [User: 18.5 ms, System: 75.0 ms]
  Range (min … max):    87.6 ms … 103.4 ms    31 runs

### unix
hyperfine --warmup 2 './run unix f_500M'
Benchmark 1: ./run unix f_500M
  Time (mean ± σ):      96.3 ms ±   5.3 ms    [User: 16.2 ms, System: 80.3 ms]
  Range (min … max):    90.6 ms … 104.8 ms    28 runs

### by char
hyperfine --warmup 2 './run char f_500M'
Benchmark 1: ./run char f_500M
  Time (mean ± σ):      3.230 s ±  0.099 s    [User: 3.157 s, System: 0.071 s]
  Range (min … max):    3.092 s …  3.399 s    10 runs
```
