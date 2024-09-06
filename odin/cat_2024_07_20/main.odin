
package main

import "core:fmt"
import "core:io"
import "core:os"

main :: proc() {
  // os.args
  fmt.printf("len = {}\n", len("ä"))
}
