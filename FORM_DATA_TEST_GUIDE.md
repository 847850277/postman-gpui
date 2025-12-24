# Form-Data POST Request Testing Guide

This guide explains how to test the form-data POST request functionality in Postman GPUI.

## Prerequisites

1. Build the application:
   ```bash
   cargo build
   ```

2. Start the test server in a separate terminal:
   ```bash
   python test_server.py
   ```
   The server will start on `http://localhost:8080`

## Testing Form-Data POST Requests

### Test Case 1: Basic Form-Data POST

1. **Launch the application:**
   ```bash
   cargo run
   ```

2. **Configure the request:**
   - **Method:** Select `POST` from the dropdown
   - **URL:** Enter `http://localhost:8080/api/submit`
   - **Body:** Click on the `Form Data` tab

3. **Add form data entries:**
   - Click "Add Row" if needed
   - Add the following key-value pairs:
     - Key: `username`, Value: `john_doe`, ✓ Enabled
     - Key: `email`, Value: `john@example.com`, ✓ Enabled
     - Key: `age`, Value: `30`, ✓ Enabled
   - Click "Add Row" to add more entries

4. **Send the request:**
   - Click the `Send` button
   - Observe the console output for the auto-added Content-Type header

5. **Verify the response:**
   - Check the response panel shows status 200
   - Verify the response body contains:
     - `"content_type": "application/x-www-form-urlencoded"`
     - `"body"` object with parsed form data:
       ```json
       {
         "username": "john_doe",
         "email": "john@example.com",
         "age": "30"
       }
       ```

6. **Check the test server console:**
   - You should see output like:
     ```
     POST /api/submit
     Content-Type: application/x-www-form-urlencoded
     Body: username=john_doe&email=john@example.com&age=30
     Parsed form data: {'username': 'john_doe', 'email': 'john@example.com', 'age': '30'}
     ```

### Test Case 2: Form-Data with Disabled Entries

1. **Add form data entries:**
   - Key: `username`, Value: `alice`, ✓ Enabled
   - Key: `password`, Value: `secret123`, ✗ Disabled (uncheck the checkbox)
   - Key: `email`, Value: `alice@example.com`, ✓ Enabled

2. **Send the request**

3. **Verify:**
   - Response should only include `username` and `email`
   - The `password` field should NOT be sent because it's disabled
   - Check application console for confirmation

### Test Case 3: Auto-Added Content-Type Header

1. **Configure form-data request** (as in Test Case 1)

2. **Check Headers section:**
   - Initially, headers section might be empty
   - After clicking Send, the app will automatically add:
     - `Content-Type: application/x-www-form-urlencoded`

3. **Verify in application console:**
   - Look for log message:
     ```
     📝 PostmanApp - Auto-added Content-Type header for form-data: application/x-www-form-urlencoded
     ```

4. **Verify in test server console:**
   - Content-Type header should be present in the received headers

### Test Case 4: Manual Content-Type Override

1. **Before configuring form data:**
   - Add a header manually:
     - Key: `Content-Type`
     - Value: `application/x-www-form-urlencoded; charset=UTF-8`
   - Click "Add"

2. **Configure form-data body:**
   - Add some form data entries

3. **Send the request**

4. **Verify:**
   - The manually added Content-Type header should be used
   - No auto-added header message in console
   - Test server should receive your custom Content-Type header

### Test Case 5: Switching Between Body Types

1. **Select Form Data tab:**
   - Add entries: `key1=value1`, `key2=value2`

2. **Switch to JSON tab:**
   - The form data should be converted to URL-encoded string

3. **Switch back to Form Data tab:**
   - Form data entries should still be there

4. **Send requests with each body type:**
   - Verify form-data uses `application/x-www-form-urlencoded`
   - Verify JSON uses `application/json` (if manually added)

## Expected Behavior Summary

1. ✅ Form Data tab displays key-value input fields
2. ✅ Add/Remove row buttons work correctly
3. ✅ Enable/disable checkboxes control which fields are sent
4. ✅ Content-Type header is automatically added when using form-data
5. ✅ Body is properly URL-encoded (`key1=value1&key2=value2`)
6. ✅ Test server correctly parses the form-data
7. ✅ Manual Content-Type headers are not overridden

## Troubleshooting

### Issue: Form data not being sent

- **Check:** Are the form data entries enabled (checkboxes checked)?
- **Check:** Is the HTTP method set to POST?
- **Check:** Are both key and value filled in for each entry?

### Issue: Test server not responding

- **Check:** Is the test server running on port 8080?
- **Check:** Is the URL correct (`http://localhost:8080`)?
- **Check:** Check test server console for error messages

### Issue: Content-Type not being added

- **Check:** Is the Body tab set to "Form Data"?
- **Check:** Look in application console for the auto-add message
- **Check:** Verify there's no manual Content-Type header conflicting

## Code Verification

The form-data support includes:

1. **UI Component** (`src/ui/components/body_input.rs`):
   - `BodyType::FormData` enum variant
   - `FormDataEntry` struct for key-value pairs
   - Form data rendering and editing UI

2. **Request Builder** (`src/app/postman_app.rs`):
   - Auto-detection of form-data body type
   - Automatic Content-Type header addition
   - URL-encoding of form data

3. **Test Server** (`test_server.py`):
   - Content-Type detection
   - Form data parsing with `urllib.parse.parse_qs`
   - Proper response formatting

## Success Criteria

✅ All test cases pass
✅ Form data is properly URL-encoded
✅ Content-Type header is automatically added
✅ Test server correctly parses the data
✅ UI is responsive and user-friendly
