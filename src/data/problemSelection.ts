// Text and controls keep their own behavior; only row whitespace selects.
export function isProblemSelectionSurface(target: EventTarget | null) {
  return target instanceof Element && !target.closest('button,input,label,a,select,textarea,h2,p,span,svg')
}
