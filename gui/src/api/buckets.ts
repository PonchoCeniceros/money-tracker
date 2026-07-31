import { call } from "./client";

export const bucketsApi = {
  deposit: (bucketId: number, fromAccountId: number, amount: number, date: string) =>
    call<number>("bucket_deposit", { bucketId, fromAccountId, amount, date }),

  /** NOT an expense — see the note in cli's `bucket withdraw` output. */
  withdraw: (bucketId: number, toAccountId: number, amount: number, date: string) =>
    call<number>("bucket_withdraw", { bucketId, toAccountId, amount, date }),
};
