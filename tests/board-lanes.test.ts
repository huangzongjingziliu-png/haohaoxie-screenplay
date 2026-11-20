import {expect,it} from 'vitest';
import type {SceneBoard} from '@/ipc/types/SceneBoard';
import {defaultCard,defaultBoard,normalizeGroupLanes} from '@/utils/scenes';

const board=(partial:Partial<SceneBoard>):SceneBoard=>({...defaultBoard(),...partial});

it('跨分区勾选成组：成员统一到组内第一个有效分区',()=>{
  const b=board({
    cards:{a:{...defaultCard(),lane:'lane1'},b:{...defaultCard(),lane:'lane2'},c:{...defaultCard(),lane:'lane2'}},
    groups:[{id:'g1',name:'组',scene_ids:['a','b','c'],collapsed:false}],
  });
  normalizeGroupLanes(b);
  expect(b.cards.a!.lane).toBe('lane1');
  expect(b.cards.b!.lane).toBe('lane1');
  expect(b.cards.c!.lane).toBe('lane1');
});

it('自动划分只返回部分成员：未返回的成员跟随组的分区',()=>{
  const b=board({
    cards:{a:{...defaultCard(),lane:'lane1'},b:{...defaultCard(),lane:'lane2'}},
    groups:[{id:'g1',name:'组',scene_ids:['a','b'],collapsed:false}],
  });
  // b 未被模型返回并被显式设成远处分区后，仍要被拉回组的分区
  normalizeGroupLanes(b);
  expect(b.cards.b!.lane).toBe('lane1');
});

it('组内都没有分区时保持未分区，不凭空空建卡片',()=>{
  const b=board({
    cards:{a:defaultCard()},
    groups:[{id:'g1',name:'组',scene_ids:['a','b'],collapsed:false}],
  });
  normalizeGroupLanes(b);
  expect(b.cards.a!.lane).toBe('');
  expect(b.cards.b).toBeUndefined();
});

it('不在任何组里的卡片不受影响',()=>{
  const b=board({cards:{x:{...defaultCard(),lane:'lane2'}}});
  normalizeGroupLanes(b);
  expect(b.cards.x!.lane).toBe('lane2');
});
