#include <pybind11/pybind11.h>
namespace py = pybind11;
using namespace py::literals;

int add(int i, int j) {
  return i+j;
}


PYBIND11_MODULE(truc, m) {
    m.doc() = "pybind11 example plugin"; // optional module docstring

    m.def("add", &add, "A function that adds two numbers", "i"_a, "j"_a);
}


