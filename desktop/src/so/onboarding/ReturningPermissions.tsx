// SPDX-License-Identifier: FSL-1.1-MIT
// The permissions screen for a returning user whose permission was revoked. (Handy's own copy sits in
// a file that also pulls in its model picker, which Smooth Operator no longer uses.)
import React from "react";
import { Permissions } from "@/components/onboarding/screens/Permissions";
import { usePermissions } from "@/components/onboarding/usePermissions";

export const ReturningPermissions: React.FC<{ onComplete: () => void }> = ({
  onComplete,
}) => {
  const permissions = usePermissions(false);
  if (!permissions.checked) return null;
  return <Permissions permissions={permissions} onContinue={onComplete} />;
};
