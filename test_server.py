#!/usr/bin/env python3
"""
简单的HTTP测试服务器，用于测试POST请求和headers功能
"""

from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import parse_qs
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
        content_type = self.headers.get('Content-Type', '')
        
        print(f"POST {self.path}")
        print(f"Headers: {dict(self.headers)}")
        print(f"Content-Type: {content_type}")
        print(f"Body: {post_data.decode('utf-8', errors='ignore')}")
        
        # Parse body based on content type
        if 'application/x-www-form-urlencoded' in content_type:
            # Parse form data
            try:
                body_str = post_data.decode('utf-8')
                parsed_form = parse_qs(body_str)
                # Convert lists to single values for simplicity
                body_json = {k: v[0] if len(v) == 1 else v for k, v in parsed_form.items()}
                print(f"Parsed form data: {body_json}")
            except Exception as e:
                print(f"Error parsing form data: {e}")
                body_json = {"raw_body": post_data.decode('utf-8', errors='ignore')}
        elif 'application/json' in content_type:
            # Parse JSON
            try:
                body_json = json.loads(post_data.decode('utf-8')) if post_data else {}
            except Exception as e:
                print(f"Error parsing JSON: {e}")
                body_json = {"raw_body": post_data.decode('utf-8', errors='ignore')}
        else:
            # Raw body
            body_json = {"raw_body": post_data.decode('utf-8', errors='ignore')}
        
        response = {
            "method": "POST",
            "path": self.path,
            "headers": dict(self.headers),
            "content_type": content_type,
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
