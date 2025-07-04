# Postman GPUI

Postman GPUI is a simple graphical user interface application for making HTTP requests, inspired by Postman. This application allows users to create, manage, and send HTTP requests and view the responses in a user-friendly manner.

## Features

- Input request details including URL, HTTP method, headers, and body.
- View responses from the server, including status codes and response bodies.
- Organize requests into collections for easy management.
- Reusable UI components for a consistent user experience.

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
│   │   ├── request_panel.rs
│   │   ├── response_panel.rs
│   │   ├── collection_panel.rs
│   │   └── components
│   │       ├── mod.rs
│   │       ├── method_selector.rs
│   │       ├── url_input.rs
│   │       ├── headers_editor.rs
│   │       └── body_editor.rs
│   ├── http             # HTTP functionalities
│   │   ├── mod.rs
│   │   ├── client.rs
│   │   ├── request.rs
│   │   └── response.rs
│   ├── models           # Data models
│   │   ├── mod.rs
│   │   ├── collection.rs
│   │   └── workspace.rs
│   ├── assets           # Application assets
│   │   └── mod.rs
│   └── utils            # Utility functions
│       ├── mod.rs
│       └── helpers.rs
├── examples             # Example usage
│   └── basic_request.rs
├── assets               # Font files
│   └── fonts
├── Cargo.toml          # Cargo configuration
└── README.md           # Project documentation
```

## Getting Started

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/postman-gpui.git
   cd postman-gpui
   ```

2. Build the project:
   ```
   cargo build
   ```

3. Run the application:
   ```
   cargo run
   ```

## Usage

- Open the application and enter the desired URL in the URL input field.
- Select the HTTP method (GET, POST, etc.) using the method selector.
- Add any necessary headers using the headers editor.
- Enter the request body if applicable.
- Click the "Send" button to make the request and view the response in the response panel.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request for any improvements or bug fixes.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.