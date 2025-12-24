# Postman GPUI

Postman GPUI is a simple graphical user interface application for making HTTP requests, inspired by Postman. This application allows users to create, manage, and send HTTP requests and view the responses in a user-friendly manner.

## Features

- Input request details including URL, HTTP method, headers, and body.
- View responses from the server, including status codes and response bodies.
- **Request History**: Click any history item to load the complete request (URL, parameters, headers, and body) back into the form
- Organize requests into collections for easy management.
- Reusable UI components for a consistent user experience.

## Request History Feature

The history list in the left sidebar shows all your previous requests. Simply **click on any history item** to:
- Load the complete URL (including query parameters)
- Load the HTTP method
- Load all headers
- Load the request body

See [FEATURE_HISTORY_LIST.md](FEATURE_HISTORY_LIST.md) for detailed documentation on the history feature.

## Project Structure

```
postman-gpui
├── src
│   ├── main.rs          # Entry point of the application
│   ├── lib.rs           # Library interface
│   ├── app              # Application logic
│   │   ├── mod.rs
│   │   └── postman_app.rs
│   ├── ui               # User interface components
│   │   ├── mod.rs
│   │   └── components
│   │       ├── mod.rs
│   │       ├── method_selector.rs  # HTTP method dropdown selector
│   │       ├── url_input.rs        # URL input field with validation
│   │       ├── header_input.rs     # Header key-value input component
│   │       ├── body_input.rs       # Request body editor with JSON support
│   │       ├── body_editor.rs      # Body editor container
│   │       └── dropdown.rs         # Reusable dropdown component
│   ├── http             # HTTP functionalities
│   │   ├── mod.rs
│   │   ├── client.rs    # HTTP client implementation
│   │   ├── request.rs   # HTTP request models
│   │   └── response.rs  # HTTP response models
│   ├── models           # Data models
│   │   ├── mod.rs
│   │   ├── collection.rs # Request collection management
│   │   └── workspace.rs  # Workspace data structures
│   ├── assets           # Application assets
│   │   └── mod.rs
│   └── utils            # Utility functions
│       ├── mod.rs
│       └── helpers.rs   # Helper utilities
├── examples             # Example usage and demos
│   ├── advanced_dropdown_example.rs
│   ├── basic_request.rs
│   ├── deferred_anchored_example.rs
│   └── text_input_demo.rs
├── assets               # Static assets
│   └── fonts           # Font files
├── postman-gpui/       # Additional examples
│   └── examples/
│       └── basic_request.rs
├── Cargo.toml          # Cargo configuration
├── Cargo.lock          # Cargo lock file
├── README.md           # Project documentation (English)
├── README-zh.md        # Project documentation (Chinese)
├── todo.md             # Development todos
└── test_server.py      # Test HTTP server for development
```

## Getting Started

1. Clone the repository:

   ```bash
   git clone https://github.com/yourusername/postman-gpui.git
   cd postman-gpui
   ```

2. Build the project:

   ```bash
   cargo build
   ```

3. Run the application:

   ```bash
   cargo run
   ```

## Usage

- Open the application and enter the desired URL in the URL input field.
- Select the HTTP method (GET, POST, etc.) using the method selector.
- Add any necessary headers using the headers editor.
- Enter the request body if applicable.
- Click the "Send" button to make the request and view the response in the response panel.

## Screenshot

![alt text](image.png)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request for any improvements or bug fixes.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.