interface Props extends JSX.Button {

}

export default function Button(props: Props) {
  return (
    <button>
      {props.children}
    </button>
  );
}
