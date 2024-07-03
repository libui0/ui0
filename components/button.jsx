export default ({ children }) => {
  const onClick = () => {
    alert("Hello, World!");
  };

  return (
    <button onClick={onClick}>
      {children}
    </button>
  );
}
