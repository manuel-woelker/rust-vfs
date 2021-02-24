#!/bin/sh

openssl req -x509 -newkey rsa:4096 -keyout private-key.key -out cert.crt -days 365 -sha256 -nodes --subj '/CN=localhost/'
