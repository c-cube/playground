
all: truc.so

INCLUDES=$(shell python -m pybind11 --includes)
truc.so: truc.cxx
	g++ -O2 -shared -std=c++11 -fPIC $(INCLUDES) -o $@ $<
