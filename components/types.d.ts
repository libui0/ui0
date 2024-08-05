


declare namespace JSX {
  interface EventHandler<T, E extends Event = Event> {
    (this: T, event: E & { target: T, currentTarget: T }): void
  }

  interface Dom<T> {
    children?: Element | Element[] | string | null;
    class?: string;
    onClick?: EventHandler<T, MouseEvent>,
  }

  interface A extends Dom<HTMLAnchorElement> {
    
  }

  interface Button extends Dom<HTMLButtonElement> {
    
  }
  
  interface Div extends Dom<HTMLDivElement> {
    
  }

  interface Input extends Dom<HTMLInputElement> {

  }

  interface HTMLElements {
    a: A,
    button: Button,
    div: Div,
    input: Input,
  }

  interface IntrinsicElements extends HTMLElements {}
}

