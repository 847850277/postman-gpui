#!/usr/bin/env python3
"""
简单的HTTP测试服务器，用于测试POST请求和headers功能
"""

from http.server import HTTPServer, BaseHTTPRequestHandler
import json
import sys

class TestHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        """处理GET请求"""
        print(f"GET {self.path}")
        print(f"Headers: {dict(self.headers)}")
        
        response = {
            "method": "GET",
            "path": self.path,
            "headers": dict(self.headers),
            "message": "GET request received successfully"
        }
        
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Access-Control-Allow-Origin', '*')
        self.end_headers()
        self.wfile.write(json.dumps(response, indent=2).encode())

    def do_POST(self):
        """处理POST请求"""
        content_length = int(self.headers.get('Content-Length', 0))
        post_data = self.rfile.read(content_length)
        
        print(f"POST {self.path}")
        print(f"Headers: {dict(self.headers)}")
        print(f"Body: {post_data.decode('utf-8', errors='ignore')}")
        
        try:
            body_json = json.loads(post_data.decode('utf-8')) if post_data else {}
        except:
            body_json = {"raw_body": post_data.decode('utf-8', errors='ignore')}
        
        response = {
            "method": "POST",
            "path": self.path,
            "headers": dict(self.headers),
            "body": body_json,
            "message": "POST request received successfully"
        }
        
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Access-Control-Allow-Origin', '*')
        self.end_headers()
        self.wfile.write(json.dumps(response, indent=2).encode())

    def do_OPTIONS(self):
        """处理OPTIONS请求（CORS预检）"""
        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type, Authorization, X-Custom-Header')
        self.end_headers()

    def log_message(self, format, *args):
        """自定义日志格式"""
        print(f"[{self.log_date_time_string()}] {format % args}")

if __name__ == '__main__':
    port = 8080
    server = HTTPServer(('localhost', port), TestHandler)
    print(f"测试服务器启动在 http://localhost:{port}")
    print("按Ctrl+C停止服务器")
    
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n服务器已停止")
        server.shutdown()
