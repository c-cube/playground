package main

import (
	"bufio"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
)

func main() {
	conn, err := net.Dial("tcp", "google.com:80")
	if err != nil {
		log.Fatal("oh no")
	}

	fmt.Fprintf(conn, "GET / HTTP/1.0\r\n\r\n")
	reader := bufio.NewReader(conn)
	resp, err := http.ReadResponse(reader, nil)
	if err != nil {
		log.Fatal("could not parse response:", err)
	}
	fmt.Println("headers:", resp.Header.Get("content-type"))

	body, err := io.ReadAll(resp.Body)

	if err != nil {
		log.Fatalf("could not read body: %v", len(body))
	}

	fmt.Printf("resp: code=%d, len=%d, body=%v\n", resp.StatusCode, len(body), string(body)[0:100])

}
