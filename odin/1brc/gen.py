import random

stations = set()
with open('weather_stations.csv', 'r') as f:
    for line in f.readlines():
        toks = line.split(';')
        if len(toks) < 2: continue
        stations.add(toks[0])

stations = list(stations)
#print(stations)

N = 100_000_000
#N = 1_000_000_000

with open('data.csv', 'w') as f:
    for i in range(N):
        idx = random.randint(0, len(stations)-1)
        station = stations[idx]
        value = random.random() * 55. - 10.
        f.write(f'{station};{value}\n')

        if i % 100_000 == 0:
            print(f'\rgenerated {i} entries', end='', flush=True)
print('\r', end='')
