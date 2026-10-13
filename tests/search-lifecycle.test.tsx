import {render,screen,fireEvent,act,waitFor,cleanup} from '@testing-library/react';
import {it,expect,afterEach} from 'vitest';
import SearchBar from '@/components/SearchBar';
import {useEditorStore} from '@/stores/editor';
import {mockBackend} from './setup';
afterEach(cleanup);
it('discards old project search results when the next project has the same revision',async()=>{mockBackend.reset();useEditorStore.setState({document:{id:'old',revision:3} as never});let reply!:(hits:unknown)=>void;mockBackend.on('search',()=>new Promise(resolve=>{reply=resolve;}));render(<SearchBar onClose={()=>{}}/>);fireEvent.change(screen.getByLabelText('搜索剧本'),{target:{value:'目标'}});await waitFor(()=>expect(mockBackend.callsTo('search')).toHaveLength(1));mockBackend.on('search',()=>[]);act(()=>useEditorStore.setState({document:{id:'new',revision:3} as never}));await act(async()=>{reply([{blockId:'old-block',text:'旧工程的目标'}]);});await waitFor(()=>expect(mockBackend.callsTo('search')).toHaveLength(2));expect(screen.queryByText('旧工程的目标')).toBeNull();});
