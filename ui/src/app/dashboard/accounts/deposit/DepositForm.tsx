"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { depositMoneyAction } from "@/actions/accounts";
import { Account } from "@/lib/accounts";
import SubmitButton from "@/components/SubmitButton";

const formatCurrency = (val: number) => {
  return new Intl.NumberFormat("en-CA", {
    style: "currency",
    currency: "CAD",
  }).format(val);
};

const accountLabel = (account: Account) => {
  const last4 = account.account_number.slice(-4);
  const type = account.account_type === "chequing" ? "Chequing" : "Savings";
  return `${type} ••${last4} — ${formatCurrency(parseFloat(account.balance))}`;
};

export default function DepositForm({
  accounts,
  initialAccountId,
}: {
  accounts: Account[];
  initialAccountId?: string;
}) {
  const router = useRouter();
  const [loading, setLoading] = useState(false);
  const [accountId, setAccountId] = useState(
    accounts.some((a) => a.account_id === initialAccountId)
      ? (initialAccountId as string)
      : accounts[0].account_id
  );
  const [amount, setAmount] = useState("");
  const [description, setDescription] = useState("");

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    const amountValue = Number(amount);
    if (!Number.isFinite(amountValue) || amountValue <= 0) {
      toast.error("Enter an amount greater than $0.00.");
      return;
    }

    setLoading(true);
    const formData = new FormData(event.currentTarget);
    try {
      const response = await depositMoneyAction(formData);
      if (response.success) {
        toast.success(response.message);
        router.push(`/dashboard/accounts/${accountId}`);
        // Don't clear `loading` here — we're navigating away, and re-enabling
        // the button while that's in flight flashes it back to "Deposit".
        return;
      }
      toast.error(response.message);
    } catch (error) {
      console.error(error);
      toast.error("An unexpected error occurred while depositing money.");
    }
    setLoading(false);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-5 w-full">
      <div className="space-y-1.5">
        <label htmlFor="accountId" className="text-xs font-semibold tracking-wide text-slate-300">
          To
        </label>
        <select
          id="accountId"
          name="accountId"
          value={accountId}
          onChange={(e) => setAccountId(e.target.value)}
          className="w-full p-3 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
        >
          {accounts.map((account) => (
            <option key={account.account_id} value={account.account_id}>
              {accountLabel(account)}
            </option>
          ))}
        </select>
      </div>

      <div className="space-y-1.5">
        <label htmlFor="amount" className="text-xs font-semibold tracking-wide text-slate-300">
          Amount
        </label>
        <div className="relative">
          <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500 text-sm">$</span>
          <input
            id="amount"
            name="amount"
            type="number"
            inputMode="decimal"
            step="0.01"
            min="0.01"
            required
            placeholder="0.00"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            className="w-full p-3 pl-7 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
          />
        </div>
      </div>

      <div className="space-y-1.5">
        <label htmlFor="description" className="text-xs font-semibold tracking-wide text-slate-300">
          Description <span className="text-slate-500 font-normal">(optional)</span>
        </label>
        <input
          id="description"
          name="description"
          type="text"
          maxLength={255}
          placeholder="Deposit"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          className="w-full p-3 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
        />
      </div>

      <SubmitButton loading={loading} loadingText="Depositing...">
        Deposit Money
      </SubmitButton>
    </form>
  );
}
