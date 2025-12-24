# History List Feature Documentation

## Overview

The History List feature allows users to view and reload previously sent HTTP requests. When you click on any history item, the complete request (URL, method, headers, and body) is automatically loaded into the request form.

## How It Works

### Viewing History

The history list is displayed in the left sidebar of the application. Each history item shows:

- **HTTP Method**: Color-coded badge (GET in green, POST in blue, etc.)
- **Timestamp**: When the request was sent (HH:MM:SS format)
- **URL**: The request URL (truncated if longer than 40 characters)
- **Additional Info** (if applicable):
  - Number of headers (e.g., "3 headers")
  - Presence of request body (e.g., "has body")

### Clicking a History Item

When you click on any history item:

1. The **HTTP method** is loaded into the method selector
2. The **complete URL** (including query parameters) is loaded into the URL input field
3. All **headers** from that request are loaded into the headers editor
4. The **request body** (if any) is loaded into the body editor

### Console Logging

For debugging and transparency, detailed information is logged to the console when:

**When clicking a history item:**
```
🔘 History item clicked:
   Index: 0
   Method: POST
   URL: https://api.example.com/users?page=1&limit=10
   Headers: 2
   Body: 45 bytes
   ➡️ Loading request into form...
```

**When the request is loaded:**
```
📋 PostmanApp - Loading request from history:
   Method: POST
   URL: https://api.example.com/users?page=1&limit=10
   Query parameters: page=1&limit=10
   Headers: 2
   Body length: 45 bytes
✅ PostmanApp - Request loaded from history successfully
   • URL loaded into URL input field
   • 2 headers loaded
   • Request body loaded
```

## Features

### Query Parameters Support

URLs with query parameters are fully supported. For example:

```
https://api.example.com/search?q=test&limit=10&sort=desc
```

When you click a history item with query parameters:
- The complete URL (including all parameters) is loaded
- Query parameters are logged separately for clarity
- The URL is ready to be modified or sent again

### Request Body Support

POST, PUT, and other requests with bodies are fully supported:
- JSON bodies
- Form data
- Raw text

The complete body is loaded back into the body editor when selecting from history.

### Headers Support

All headers from the original request are restored:
- Headers are re-added to the headers list
- All headers are enabled by default when loaded from history
- Custom headers like Authorization, Content-Type, etc. are preserved

### Visual Indicators

History items show contextual information:
- "2 headers" - indicates the request had 2 headers
- "has body" - indicates the request had a body
- "3 headers • has body" - combination of both

### Selection State

The currently selected history item is highlighted with a light blue background, making it easy to see which request is currently loaded.

## History Ordering

History items are displayed in reverse chronological order (newest first):
- Most recent requests appear at the top
- Older requests appear below
- Maximum of 50 entries are kept (configurable via `DEFAULT_MAX_HISTORY_ENTRIES`)

## Example Use Cases

### 1. Re-sending a Previous Request

1. Click on a history item
2. The request is loaded into the form
3. Click "Send" to execute the same request again

### 2. Modifying a Previous Request

1. Click on a history item to load it
2. Modify the URL, headers, or body as needed
3. Click "Send" to execute the modified request

### 3. Comparing Requests

1. Click one history item to view its details
2. Note the URL and parameters
3. Click another history item to compare

### 4. Testing with Query Parameters

1. Send a request with query parameters: `https://api.example.com/users?page=1`
2. The request appears in history
3. Click the history item to reload it
4. Modify the query parameter: `?page=2`
5. Send again to test different parameters

## Implementation Details

### Code Structure

- **History List Component**: `src/ui/components/history_list.rs`
- **History Model**: `src/models/history.rs`
- **Request Model**: `src/models/request.rs`
- **Main App Integration**: `src/app/postman_app.rs`

### Event Flow

```
User clicks history item
    ↓
history_list.rs: on_item_clicked()
    ↓
Emits HistoryListEvent::RequestSelected(request)
    ↓
postman_app.rs: on_history_selected()
    ↓
Loads request into form components:
    - method_selector.set_selected_method()
    - url_input.set_url()
    - Updates headers vector
    - body_input.set_content()
    ↓
UI updates to show loaded request
```

### Testing

Comprehensive unit tests are available in `src/models/history.rs`:

- `test_add_history_entry` - Basic history entry creation
- `test_history_with_query_parameters` - URL query parameter handling
- `test_history_with_body` - Request body preservation
- `test_history_order` - Verify newest-first ordering
- `test_history_max_entries` - Test entry limit
- `test_history_clear` - Test clearing history
- `test_history_entry_display_name` - Test display formatting

Run tests with: `cargo test models::history`

## Future Enhancements

Potential improvements for future versions:

1. **Persistent History**: Save history to disk to survive app restarts
2. **Search/Filter**: Search history by URL, method, or timestamp
3. **History Export**: Export history to JSON or other formats
4. **Request Comparison**: Side-by-side comparison of two requests
5. **Favorites**: Mark certain requests as favorites
6. **Request Naming**: Allow users to give custom names to requests
7. **Response Preview**: Show response status in history list
8. **Delete Individual Items**: Remove specific history entries

## Troubleshooting

### History item not loading correctly

**Symptom**: Clicking a history item doesn't load the request

**Solution**: Check the console for error messages. The logging should show exactly what's being loaded.

### Query parameters not showing

**Symptom**: Query parameters in URL are missing

**Solution**: Query parameters are part of the URL string. Check that the URL in the history includes the `?` character and parameters.

### History list empty

**Symptom**: No items shown in history

**Solution**: Send at least one request using the "Send" button. History is only created for successfully sent requests.

## Contributing

To contribute improvements to the history feature:

1. Review the code in `src/ui/components/history_list.rs`
2. Add tests for new functionality in `src/models/history.rs`
3. Ensure all existing tests pass
4. Update this documentation with any new features
