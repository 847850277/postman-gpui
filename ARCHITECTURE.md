# JsonTextElement Architecture

## Component Structure

```
BodyInput (Main Component)
    ├── EntityInputHandler (Trait Implementation)
    │   ├── text_for_range() - Returns text for a UTF-16 range
    │   ├── selected_text_range() - Returns current selection
    │   ├── marked_text_range() - Returns IME composition range
    │   ├── replace_text_in_range() - Replaces text in a range
    │   ├── replace_and_mark_text_in_range() - IME composition
    │   ├── bounds_for_range() - Returns bounds for text range
    │   └── character_index_for_point() - Mouse position to index
    │
    ├── JsonTextElement (Custom Element)
    │   ├── request_layout() - Calculates required space
    │   ├── prepaint() - Prepares rendering data
    │   │   ├── Splits text into lines
    │   │   ├── Shapes each line with TextSystem
    │   │   ├── Calculates cursor position
    │   │   └── Calculates selection rectangles
    │   │
    │   └── paint() - Renders to screen
    │       ├── Registers InputHandler
    │       ├── Paints selection highlights
    │       ├── Paints text lines
    │       └── Paints cursor (if focused)
    │
    ├── Action Handlers
    │   ├── Navigation: json_left, json_right, json_up, json_down
    │   ├── Selection: json_select_left, json_select_right, etc.
    │   ├── Editing: json_backspace, json_delete, json_enter
    │   ├── Clipboard: json_copy, json_paste, json_cut
    │   └── Movement: json_home, json_end, json_select_all
    │
    └── Helper Methods
        ├── UTF-16 Conversion
        │   ├── json_offset_to_utf16()
        │   ├── json_offset_from_utf16()
        │   ├── json_range_to_utf16()
        │   └── json_range_from_utf16()
        │
        ├── Line Navigation
        │   ├── json_line_start()
        │   ├── json_line_end()
        │   ├── json_offset_for_line_up()
        │   └── json_offset_for_line_down()
        │
        ├── Selection Management
        │   ├── json_move_to()
        │   ├── json_select_to()
        │   └── json_cursor_offset()
        │
        ├── Text Boundaries
        │   ├── json_previous_boundary()
        │   └── json_next_boundary()
        │
        └── Mouse Interaction
            ├── json_index_for_mouse_position()
            ├── json_on_mouse_down()
            ├── json_on_mouse_up()
            └── json_on_mouse_move()
```

## Data Flow

### Text Input Flow
```
User Types → System IME → EntityInputHandler::replace_text_in_range()
                ↓
         json_replace_text_in_range()
                ↓
    Update json_content & json_selected_range
                ↓
         Emit ValueChanged Event
                ↓
           Trigger Repaint
```

### Cursor Movement Flow
```
User Presses Arrow Key → Action Handler (json_left/right/up/down)
                ↓
     Calculate New Offset (using boundary/line methods)
                ↓
         json_move_to() or json_select_to()
                ↓
     Update json_selected_range
                ↓
           Trigger Repaint
```

### Mouse Selection Flow
```
Mouse Down → json_on_mouse_down()
    ↓
json_index_for_mouse_position()
    ↓
json_move_to() or json_select_to()
    ↓
Set json_is_selecting = true

Mouse Move (while selecting) → json_on_mouse_move()
    ↓
json_index_for_mouse_position()
    ↓
json_select_to()
    ↓
Trigger Repaint

Mouse Up → json_on_mouse_up()
    ↓
Set json_is_selecting = false
```

### Rendering Flow
```
GPUI Render Cycle
    ↓
request_layout()
    ├── Read content line count
    ├── Calculate height = line_count × line_height
    └── Return layout requirements
    ↓
prepaint()
    ├── Read content from BodyInput
    ├── Split into lines
    ├── Shape each line with TextSystem
    ├── Calculate cursor position
    │   ├── Find line index for cursor offset
    │   ├── Find offset within line
    │   └── Calculate X position using ShapedLine
    └── Calculate selection rectangles
        ├── Find start/end line indices
        ├── For single line: One rectangle
        └── For multi-line: Multiple rectangles
    ↓
paint()
    ├── Register ElementInputHandler
    ├── Paint selection quads (background)
    ├── Paint text lines (foreground)
    ├── Paint cursor (if focused)
    └── Save layout & bounds for mouse interaction
```

## State Management

### Persistent State (in BodyInput)
```
json_content: String                  // The actual JSON text
json_selected_range: Range<usize>     // Current selection (UTF-8 offsets)
json_selection_reversed: bool         // Direction of selection extension
json_marked_range: Option<Range>      // IME composition range
json_last_layout: Vec<ShapedLine>     // Cached line layouts
json_last_bounds: Option<Bounds>      // Cached element bounds
json_is_selecting: bool               // Mouse drag state
```

### Transient State (in JsonPrepaintState)
```
lines: Vec<ShapedLine>      // Shaped text lines for rendering
cursor: Option<PaintQuad>   // Cursor rectangle (if showing)
selection: Vec<PaintQuad>   // Selection highlight rectangles
```

## Key Algorithms

### Line Index Calculation
```
find_line_for_offset(content: &str, offset: usize) -> (line_idx, offset_in_line)
    current_offset = 0
    for each line in content.lines():
        if current_offset + line.len() >= offset:
            return (line_idx, offset - current_offset)
        current_offset += line.len() + 1  // +1 for newline
```

### Multi-line Selection Rendering
```
calculate_selection_quads(content, lines, range, bounds, line_height)
    Find start_line, start_offset from range.start
    Find end_line, end_offset from range.end
    
    if start_line == end_line:
        Create one quad from start_offset to end_offset
    else:
        for each line from start_line to end_line:
            if line == start_line:
                Quad from start_offset to end of line
            else if line == end_line:
                Quad from start of line to end_offset
            else:
                Quad for entire line
```

### UTF-16 Conversion
```
json_offset_to_utf16(utf8_offset) -> utf16_offset
    utf16_offset = 0
    utf8_count = 0
    
    for each char in content:
        if utf8_count >= utf8_offset:
            break
        utf8_count += char.len_utf8()
        utf16_offset += char.len_utf16()
    
    return utf16_offset
```

## Integration Points

### With GPUI Framework
- Implements `Element` trait for custom rendering
- Implements `EntityInputHandler` for system input integration
- Uses `TextSystem` for text shaping and layout
- Uses `FocusHandle` for focus management
- Uses actions system for keyboard shortcuts

### With BodyInput Component
- JSON type check in all action handlers
- Integrated into render() method
- Shares focus_handle with parent component
- Emits ValueChanged events on content modification

### With System
- IME support through EntityInputHandler
- Clipboard integration (read/write)
- Mouse event handling
- Keyboard event handling
- Focus management
