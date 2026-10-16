// An Edit asked for from outside the Accounts view (the rail's account
// menu): the account's key. The view opens that row's editor once it has
// the row, then clears this.

export const editRequest = $state({ key: null as string | null });
