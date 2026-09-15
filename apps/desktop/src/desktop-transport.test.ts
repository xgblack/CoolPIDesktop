import {describe,expect,it} from 'vitest';
import {desktopTransport} from './desktop-transport';
import {
  gitWrites,
  host,
  hostCapabilities,
  localProjects,
  modelConfig,
  openInApp,
  records,
  resources,
  taskRuntime,
  terminal,
  titlePrompt,
  trajectory,
} from './host';

describe('desktop transport',()=>{
  it('maps the current host surface without changing legacy call ownership',()=>{
    expect(desktopTransport.capabilities).toBe(hostCapabilities);
    expect(desktopTransport.projects.chooseFolders).toBe(localProjects.choose);
    expect(desktopTransport.tasks.listRecords).toBe(records.list);
    expect(desktopTransport.runtime.snapshot).toBe(host.snapshot);
    expect(desktopTransport.runtime.listManaged).toBe(taskRuntime.list);
    expect(desktopTransport.files.preview).toBe(resources.preview);
    expect(desktopTransport.git.commit).toBe(gitWrites.commit);
    expect(desktopTransport.terminals).toBe(terminal);
    expect(desktopTransport.models).toBe(modelConfig);
    expect(desktopTransport.trajectory).toBe(trajectory);
    expect(desktopTransport.titlePrompt).toBe(titlePrompt);
    expect(desktopTransport.openInApp).toBe(openInApp);
  });
});
