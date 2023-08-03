#!/bin/sh
response_code=$(printf 'GET / HTTP/1.1\r\nConnection: close\r\n\r\n' | nc -w3 127.0.0.1 8080 | head -n1 | cut -d' ' -f2)
echo "$response_code"
[ "$response_code" = 200 ]
