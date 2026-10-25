# A diagram

```mermaid alt="Login flow: the user submits credentials, the server checks them"
flowchart LR
  A[Submit] --> B{Valid?}
  B -->|yes| C[Dashboard]
  B -->|no| D[Error]
```

Text after the diagram.
