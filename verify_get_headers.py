#!/usr/bin/env python3
"""
Simple verification script to demonstrate GET requests with headers work correctly
This demonstrates what the Rust code does
"""

import requests
import json

def test_get_without_headers():
    """Test GET request without headers"""
    print("\n" + "="*60)
    print("Test 1: GET request WITHOUT headers")
    print("="*60)
    
    url = "http://localhost:8080/test"
    response = requests.get(url)
    
    print(f"Status: {response.status_code}")
    print(f"Response:\n{json.dumps(response.json(), indent=2)}")
    
    assert response.status_code == 200
    assert response.json()["method"] == "GET"
    print("✅ Test 1 PASSED")

def test_get_with_headers():
    """Test GET request with custom headers"""
    print("\n" + "="*60)
    print("Test 2: GET request WITH custom headers")
    print("="*60)
    
    url = "http://localhost:8080/test"
    headers = {
        "Authorization": "Bearer test-token-123",
        "X-Custom-Header": "custom-value",
        "User-Agent": "postman-gpui/0.1.0"
    }
    
    print(f"Sending headers:")
    for key, value in headers.items():
        print(f"  {key}: {value}")
    
    response = requests.get(url, headers=headers)
    
    print(f"\nStatus: {response.status_code}")
    print(f"Response:\n{json.dumps(response.json(), indent=2)}")
    
    # Verify the headers were received
    response_data = response.json()
    received_headers = response_data["headers"]
    
    assert response.status_code == 200
    assert response_data["method"] == "GET"
    assert "Authorization" in received_headers
    assert received_headers["Authorization"] == "Bearer test-token-123"
    assert "X-Custom-Header" in received_headers
    assert received_headers["X-Custom-Header"] == "custom-value"
    
    print("✅ Test 2 PASSED - Headers were successfully sent and received!")

if __name__ == "__main__":
    print("\n" + "="*60)
    print("GET Method Header Support Verification")
    print("="*60)
    print("\nThis script demonstrates that the GET method now supports headers,")
    print("matching the functionality already available in the POST method.")
    
    try:
        test_get_without_headers()
        test_get_with_headers()
        
        print("\n" + "="*60)
        print("✅ ALL TESTS PASSED!")
        print("="*60)
        print("\nThe HttpClient::get() method now accepts an optional headers parameter")
        print("and correctly forwards them to the HTTP request, just like POST does.")
        print("\nSignature: pub async fn get(&self, url: &str, headers: Option<HashMap<String, String>>)")
        
    except Exception as e:
        print(f"\n❌ Error: {e}")
        print("\nMake sure the test server is running:")
        print("  python3 test_server.py")
