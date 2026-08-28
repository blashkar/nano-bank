"use client";

import React, { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { ArrowLeftRight } from "lucide-react";
import { transferMoneyAction } from "@/actions/accounts";
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

export default function TransferForm({
  accounts,
  initialFromAccountId,
}: {
  accounts: Account[];
  initialFromAccountId?: string;
}) {
  const router = useRouter();
  const [loading, setLoading] = useState(false);
  const [fromAccountId, setFromAccountId] = useState(
    accounts.some((a) => a.account_id === initialFromAccountId)
      ? (initialFromAccountId as string)
      : accounts[0].account_id
  );
  const [toAccountId, setToAccountId] = useState(
    accounts.find((a) => a.account_id !== fromAccountId)?.account_id ?? accounts[0].account_id
  );
  const [amount, setAmount] = useState("");
  const [description, setDescription] = useState("");
  // One key for the lifetime of this form mount: a double-click or a retry
  // after a dropped response reuses it, so the server collapses the repeat
  // into the original transfer instead of moving the money twice.
  const [idempotencyKey] = useState(() => crypto.randomUUID());

  const fromAccount = accounts.find((a) => a.account_id === fromAccountId);
  const toOptions = useMemo(
    () => accounts.filter((a) => a.account_id !== fromAccountId),
    [accounts, fromAccountId]
  );

  const handleFromChange = (id: string) => {
    setFromAccountId(id);
    if (toAccountId === id) {
      setToAccountId(accounts.find((a) => a.account_id !== id)?.account_id ?? id);
    }
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    if (fromAccountId === toAccountId) {
      toast.error("The from and to accounts must be different.");
      return;
    }
    const amountValue = Number(amount);
    if (!Number.isFinite(amountValue) || amountValue <= 0) {
      toast.error("Enter an amount greater than $0.00.");
      return;
    }

    setLoading(true);
    const formData = new FormData(event.currentTarget);
    try {
      const response = await transferMoneyAction(formData);
      if (response.success) {
        toast.success(response.message);
        router.push(`/dashboard/accounts/${fromAccountId}`);
        // Don't clear `loading` here — we're navigating away, and re-enabling
        // the button while that's in flight flashes it back to "Transfer".
        return;
      }
      toast.error(response.message);
    } catch (error) {
      console.error(error);
      toast.error("An unexpected error occurred while transferring money.");
    }
    setLoading(false);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-5 w-full">
      <input type="hidden" name="idempotencyKey" value={idempotencyKey} />

      <div className="space-y-1.5">
        <label htmlFor="fromAccountId" className="text-xs font-semibold tracking-wide text-slate-300">
          From
        </label>
        <select
          id="fromAccountId"
          name="fromAccountId"
          value={fromAccountId}
          onChange={(e) => handleFromChange(e.target.value)}
          className="w-full p-3 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
        >
          {accounts.map((account) => (
            <option key={account.account_id} value={account.account_id}>
              {accountLabel(account)}
            </option>
          ))}
        </select>
      </div>

      <div className="flex justify-center text-slate-500">
        <ArrowLeftRight className="w-4 h-4" />
      </div>

      <div className="space-y-1.5">
        <label htmlFor="toAccountId" className="text-xs font-semibold tracking-wide text-slate-300">
          To
        </label>
        <select
          id="toAccountId"
          name="toAccountId"
          value={toAccountId}
          onChange={(e) => setToAccountId(e.target.value)}
          className="w-full p-3 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
        >
          {toOptions.map((account) => (
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
        {fromAccount && (
          <p className="text-xs text-slate-500">
            Available balance: {formatCurrency(parseFloat(fromAccount.available_balance))}
          </p>
        )}
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
          placeholder="Transfer between own accounts"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          className="w-full p-3 rounded-lg border border-slate-700 bg-slate-900/50 text-sm text-white placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-nanobank-blue-sky/60"
        />
      </div>

      <p className="text-xs text-slate-500">A small flat fee applies and is charged to the source account.</p>

      <SubmitButton loading={loading} loadingText="Transferring...">
        Transfer Money
      </SubmitButton>
    </form>
  );
}
