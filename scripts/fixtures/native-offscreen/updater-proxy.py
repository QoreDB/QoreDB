#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Serve a staged updater manifest and signed AppImage inside a test container.

Only localhost clients and CONNECT github.com:443 are accepted. No requests are
forwarded. The test CA must be trusted only by the disposable application, never
by the host. Package authenticity still uses QoreDB's unchanged updater key.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import ssl

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('profile', type=Path)
parser.add_argument('candidate', type=Path)
args = parser.parse_args()
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(args.profile / 'proxy-server.crt', args.profile / 'proxy-server.key')


class Handler(BaseHTTPRequestHandler):
    def do_CONNECT(self):
        if self.path != 'github.com:443':
            self.send_error(403)
            return
        self.send_response(200, 'Connection Established')
        self.end_headers()
        self.connection = context.wrap_socket(self.connection, server_side=True)
        self.rfile = self.connection.makefile('rb')
        self.wfile = self.connection.makefile('wb')
        self.close_connection = True
        self.handle_one_request()

    def do_GET(self):
        if self.path == '/QoreDB/QoreDB/releases/latest/download/latest.json':
            manifest = (args.profile / 'updater.json').read_bytes()
            assert json.loads(manifest)['version'] == '0.1.40'
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(manifest)))
            self.end_headers()
            self.wfile.write(manifest)
        elif self.path == '/candidate.AppImage':
            self.send_response(200)
            self.send_header('Content-Length', str(args.candidate.stat().st_size))
            self.end_headers()
            with args.candidate.open('rb') as file:
                while block := file.read(1024 * 1024):
                    self.wfile.write(block)
        else:
            self.send_error(404)


ThreadingHTTPServer(('127.0.0.1', 9880), Handler).serve_forever()
