/** Generated presentation metadata must never become authored source text. */
export function stripAutoContinuation(text: string, generated?: boolean): string {
  return generated ? text.replace(/[,，]\s*CONT['’]?D(?=[）)]$)/i, '').replace(/\s*[（(]CONT['’]?D[）)]$/i, '') : text;
}
