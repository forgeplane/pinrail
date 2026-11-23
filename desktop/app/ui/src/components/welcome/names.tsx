// Names in a sentence: "A", "A and B", "A, B and C".

import { Fragment, type ReactNode } from "react";

export function listed(names: string[], strong = false): ReactNode {
  return names.map((name, i) => (
    <Fragment key={name}>
      {i === 0 ? "" : i === names.length - 1 ? " and " : ", "}
      {strong ? <b>{name}</b> : name}
    </Fragment>
  ));
}
