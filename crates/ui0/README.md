# ui0

Tool with included UI components

**Currently under construction**

```shell
# cli - with existing routes, include needed components to bundle
ui0
```

```typescript jsx
// routes.tsx
export default function Routes() {
    return (
        <Router>
            <Route match={/^\/\?path=(?<path>[a-z]+)/} />
            <Route path="/user/:id" />
        </Router>
    )
}
```

```rust
// handler.rs
fn render() {
    let id = 5;
    render!("/user/{id}"); // implicitly init router
    
    // or
    
    let x = ui0::get_routes(); // explicit call
    x.render("/?path=test")
}
```

