// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, lazy, Suspense, useEffect, useState } from 'react';

const QueryLibraryModal = lazy(() =>
  import('./QueryLibraryModal').then(module => ({ default: module.QueryLibraryModal }))
);

export function DeferredQueryLibraryModal(props: ComponentProps<typeof QueryLibraryModal>) {
  const [hasOpened, setHasOpened] = useState(false);
  useEffect(() => {
    if (props.isOpen) setHasOpened(true);
  }, [props.isOpen]);

  // Retain search and folder selection across closes after the first load.
  if (!props.isOpen && !hasOpened) return null;
  return (
    <Suspense fallback={null}>
      <QueryLibraryModal {...props} />
    </Suspense>
  );
}
