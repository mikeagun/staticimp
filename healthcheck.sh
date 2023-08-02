#!/bin/sh
echo -e 'HEAD / HTTP/1.1\r\nUser-Agent: nc/0.0.1\r\nHost: staticimp\r\nAccept: */*\r\n\r\n' | nc -w3 127.0.0.1 8080 | head -n1 | cut -d' ' -f2
