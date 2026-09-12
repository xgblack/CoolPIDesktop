import { useCallback, useEffect, useState } from 'react';
import { trajectory, hostError } from '../../../host';
import type { TaskSnapshot, TrajectoryPage, TrajectoryRecord } from '../../../../../../packages/host-contract/src';

export function useTrajectory(taskId: string, run?: TaskSnapshot) {
  const [data, setData] = useState<TrajectoryPage>({ records: [], nextCursor: null, totalRecords: 0, revision: '', warnings: [] });
  const [loading, setLoading] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [error, setError] = useState<ReturnType<typeof hostError> | null>(null);
  const refresh = useCallback(async () => {
    if (!taskId) return;
    setLoading(true); setError(null);
    try { setData(await trajectory.read(taskId)); } catch (e) { setError(hostError(e)); }
    finally { setLoading(false); }
  }, [taskId]);
  useEffect(() => { void refresh(); }, [refresh, run?.seq, run?.status]);
  const loadOlder = useCallback(async () => { setLoadingOlder(false); }, []);
  const records: TrajectoryRecord[] = data.records;
  return { ...data, records, loading, loadingOlder, hasMore: !!data.nextCursor, error, refresh, loadOlder };
}
